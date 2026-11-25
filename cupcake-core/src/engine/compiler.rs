//! Compiler module - Invokes OPA to compile policies into a unified WASM module.
//!
//! Uses `cupcake.system.evaluate` as the single aggregation entrypoint.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;
use tracing::{debug, error, info};

use super::PolicyUnit;

mod opa_binary;

pub use opa_binary::find_opa_binary;

/// Compile all policies into a single unified WASM module using OPA
pub async fn compile_policies(
    policies: &[PolicyUnit],
    opa_path_override: Option<PathBuf>,
) -> Result<Vec<u8>> {
    compile_policies_with_namespace(policies, "cupcake.system", opa_path_override, None).await
}

/// Compile policies with a specific namespace for the entrypoint
///
/// # Arguments
/// * `policies` - List of policy units to compile
/// * `namespace` - Namespace for the entrypoint (e.g., "cupcake.system")
/// * `opa_path_override` - Optional path to OPA binary
/// * `cupcake_dir` - Optional path to .cupcake directory for locating helpers at root level
pub async fn compile_policies_with_namespace(
    policies: &[PolicyUnit],
    namespace: &str,
    opa_path_override: Option<PathBuf>,
    cupcake_dir: Option<&Path>,
) -> Result<Vec<u8>> {
    if policies.is_empty() {
        bail!("No policies to compile");
    }

    info!(
        "Compiling {} policies into unified WASM module",
        policies.len()
    );

    // Create a temporary directory for the compilation that auto-cleans on drop
    let temp_dir = TempDir::new().context("Failed to create temp directory for OPA compilation")?;

    // DON'T canonicalize on Windows to avoid UNC paths (\\?\C:\...)
    // OPA has path handling bugs with both UNC paths and drive letters
    let temp_path = temp_dir.path();
    debug!("Using temp directory: {:?}", temp_path);

    // Write all policies to the temp directory, preserving directory structure
    debug!("Copying {} policies to temp dir", policies.len());

    let policies_root = find_policies_root(policies)?;
    debug!("Policies root: {:?}", policies_root);

    let helpers_src = helpers_source_dir(cupcake_dir, policies_root);
    copy_helpers(&helpers_src, temp_path).await?;

    for policy in policies.iter() {
        stage_policy(policy, policies_root, temp_path, namespace).await?;
    }

    // Build the OPA command for Hybrid Model
    // Single entrypoint: cupcake.system.evaluate
    let opa_path = find_opa_binary(opa_path_override)?;
    debug!("Using OPA binary: {:?}", opa_path);
    let bundle_path = temp_path.join("bundle.tar.gz");
    let mut opa_cmd = build_opa_command(&opa_path, namespace, temp_path, &bundle_path)?;

    info!("Executing OPA build command...");
    debug!("OPA command: {:?}", opa_cmd);

    // Execute the command - this MUST work or we fail
    let output = opa_cmd
        .output()
        .context("Failed to execute OPA command. Is the OPA CLI installed and in your PATH?")?;

    if !output.status.success() {
        bail!(
            "OPA compilation failed: {}",
            describe_opa_failure(&opa_cmd, &opa_path, &output)
        );
    }

    info!("OPA compilation successful");

    // Extract the WASM module from the bundle
    let wasm_bytes = extract_wasm_from_bundle(&bundle_path)
        .await
        .context("Failed to extract WASM from bundle")?;

    info!("Extracted WASM module: {} bytes", wasm_bytes.len());

    // Debug: Save WASM to temp file for inspection
    // Use a different temp file since the compilation temp_dir will be auto-deleted
    let debug_wasm_path =
        std::env::temp_dir().join(format!("cupcake-debug-{}.wasm", std::process::id()));
    tokio::fs::write(&debug_wasm_path, &wasm_bytes).await?;
    debug!("Saved WASM to {:?} for debugging", debug_wasm_path);

    Ok(wasm_bytes)
}

/// Find the common `policies` directory the policy files live under.
///
/// Walks up from the first policy's path to a directory named `policies`,
/// falling back to that path's parent.
fn find_policies_root(policies: &[PolicyUnit]) -> Result<&Path> {
    let Some(first) = policies.first() else {
        bail!("No policies to determine root from");
    };
    let first_policy_path = &first.path;
    let mut current = first_policy_path.parent();
    while let Some(parent) = current {
        if parent.file_name() == Some(std::ffi::OsStr::new("policies")) {
            break;
        }
        current = parent.parent();
    }
    current
        .or_else(|| first_policy_path.parent())
        .context("First policy path has no parent directory")
}

/// Choose the helpers directory to copy (required by refactored builtins).
///
/// New structure: helpers at `.cupcake/helpers/` (`cupcake_dir/helpers`).
/// Fallback: helpers at `.cupcake/policies/helpers/` (`policies_root/helpers`).
fn helpers_source_dir(cupcake_dir: Option<&Path>, policies_root: &Path) -> PathBuf {
    let Some(cupcake) = cupcake_dir else {
        return policies_root.join("helpers");
    };
    let root_helpers = cupcake.join("helpers");
    if root_helpers.exists() && root_helpers.is_dir() {
        debug!("Using helpers from cupcake root: {:?}", root_helpers);
        root_helpers
    } else {
        debug!("No helpers at cupcake root, checking policies root");
        policies_root.join("helpers")
    }
}

/// Copy every `.rego` file of the helpers directory into `temp_path/helpers`.
///
/// Does nothing when `helpers_src` is not a directory.
async fn copy_helpers(helpers_src: &Path, temp_path: &Path) -> Result<()> {
    if !(helpers_src.exists() && helpers_src.is_dir()) {
        return Ok(());
    }
    debug!("Copying helpers directory: {:?}", helpers_src);
    let helpers_dest = temp_path.join("helpers");
    tokio::fs::create_dir_all(&helpers_dest).await?;

    let mut helpers_dir = tokio::fs::read_dir(helpers_src).await?;
    while let Some(entry) = helpers_dir.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("rego") {
            let file_name = path.file_name().unwrap();
            let dest_path = helpers_dest.join(file_name);
            tokio::fs::copy(&path, &dest_path).await?;
            debug!("Copied helper: {:?} -> {:?}", path, dest_path);
        }
    }
    Ok(())
}

/// Write one policy into the temp tree, moving it to the global namespace
/// when compiling for `cupcake.global` and its package is not yet global.
async fn stage_policy(
    policy: &PolicyUnit,
    policies_root: &Path,
    temp_path: &Path,
    namespace: &str,
) -> Result<()> {
    // Get the relative path from the policies root
    let relative_path = match policy.path.strip_prefix(policies_root) {
        Ok(relative) => relative,
        Err(_) => Path::new(
            policy
                .path
                .file_name()
                .context("Policy path has no file name")?,
        ),
    };

    let dest_path = temp_path.join(relative_path);

    // Create parent directories if needed (for system/ subdirectory)
    if let Some(parent) = dest_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut content = tokio::fs::read_to_string(&policy.path)
        .await
        .context(format!("Failed to read policy {:?}", policy.path))?;

    // If this is for global namespace and the package needs transformation
    if namespace.starts_with("cupcake.global") && !content.contains("package cupcake.global") {
        content = content
            .replace(
                "package cupcake.policies",
                "package cupcake.global.policies",
            )
            .replace("package cupcake.system", "package cupcake.global.system");
        debug!(
            "Transformed policy {} to global namespace",
            policy.package_name
        );
    }

    tokio::fs::write(&dest_path, &content)
        .await
        .context(format!("Failed to write policy {dest_path:?}"))?;

    debug!(
        "Policy {} content preview (first 200 chars): {}",
        policy.package_name,
        &content.chars().take(200).collect::<String>()
    );
    debug!(
        "Wrote policy {} to temp: {:?}",
        policy.package_name, dest_path
    );
    Ok(())
}

/// Build the `opa build` command for the single Hybrid Model entrypoint.
///
/// On Windows the policy directory is passed as a `file://` URL (OPA bug
/// #4174 strips drive letters) and the bundle path is relative to a working
/// directory of `temp_path`, because OPA cannot write to `file://` URLs.
fn build_opa_command(
    opa_path: &Path,
    namespace: &str,
    temp_path: &Path,
    bundle_path: &Path,
) -> Result<Command> {
    let mut opa_cmd = Command::new(opa_path);
    opa_cmd
        .arg("build")
        .arg("-t")
        .arg("wasm") // Target WASM
        .arg("-O")
        .arg("2"); // Optimization level 2

    // This entrypoint collects all decision verbs across all policies
    let entrypoint = format!("{}/evaluate", namespace.replace('.', "/"));
    opa_cmd.arg("-e").arg(&entrypoint);
    debug!("Added single Hybrid Model entrypoint: {}", entrypoint);

    let temp_path_str = temp_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Failed to convert temp path to string"))?;
    let temp_path_arg = if cfg!(windows) {
        let url_path = temp_path_str.replace('\\', "/");
        format!("file:///{url_path}")
    } else {
        temp_path_str.to_string()
    };
    debug!("Temp path for OPA: {:?}", temp_path_arg);
    opa_cmd.arg(&temp_path_arg);

    let bundle_path_str = bundle_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Failed to convert bundle path to string"))?;
    let bundle_path_arg = if cfg!(windows) {
        opa_cmd.current_dir(temp_path);
        "bundle.tar.gz".to_string()
    } else {
        bundle_path_str.to_string()
    };
    debug!("Bundle path: {:?}", bundle_path_arg);
    opa_cmd.arg("-o").arg(&bundle_path_arg);
    Ok(opa_cmd)
}

/// Log a failed OPA run and summarise its output for the error message.
fn describe_opa_failure(
    opa_cmd: &Command,
    opa_path: &Path,
    output: &std::process::Output,
) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    error!(
        "OPA command failed: {:?}\nStderr: {}\nStdout: {}",
        opa_cmd, stderr, stdout
    );
    log_windows_opa_diagnostics(opa_path, output);

    if !stderr.is_empty() {
        format!("stderr: {stderr}")
    } else if !stdout.is_empty() {
        format!("stdout: {stdout}")
    } else {
        format!("No output from OPA. Exit code: {:?}", output.status.code())
    }
}

/// Provide more context on Windows, where OPA path handling is fragile.
#[cfg(windows)]
fn log_windows_opa_diagnostics(opa_path: &Path, output: &std::process::Output) {
    error!("Windows-specific debugging:");
    error!("Exit code: {:?}", output.status.code());
    error!("OPA binary path used: {}", opa_path.display());

    // Try to check if OPA exists
    if !opa_path.exists() {
        error!("OPA binary does not exist at: {}", opa_path.display());
    } else {
        error!("OPA binary exists at: {}", opa_path.display());
    }
}

/// Windows-only diagnostics; nothing to log elsewhere.
#[cfg(not(windows))]
fn log_windows_opa_diagnostics(_opa_path: &Path, _output: &std::process::Output) {}

/// Extract the policy.wasm file from the OPA bundle
async fn extract_wasm_from_bundle(bundle_path: &Path) -> Result<Vec<u8>> {
    // OPA creates a tar.gz bundle, we need to extract policy.wasm from it

    let bundle_bytes = tokio::fs::read(bundle_path)
        .await
        .context("Failed to read bundle file")?;

    // Create temp directory for extraction that auto-cleans on drop
    let temp_dir =
        TempDir::new().context("Failed to create temp directory for bundle extraction")?;
    let extract_path = temp_dir.path();

    // Write bundle to temp location
    let temp_bundle = extract_path.join("bundle.tar.gz");
    tokio::fs::write(&temp_bundle, &bundle_bytes).await?;

    // Extract using tar command
    let output = Command::new("tar")
        .arg("-xzf")
        .arg(&temp_bundle)
        .arg("-C")
        .arg(extract_path)
        .output()
        .context("Failed to extract tar bundle")?;

    if !output.status.success() {
        bail!(
            "Failed to extract bundle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Read the policy.wasm file
    let wasm_path = extract_path.join("policy.wasm");
    if !wasm_path.exists() {
        bail!("No policy.wasm found in OPA bundle");
    }

    Ok(tokio::fs::read(wasm_path).await?)
}

#[cfg(test)]
mod tests;
