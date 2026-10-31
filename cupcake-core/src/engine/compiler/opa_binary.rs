//! Discovery and validation of the OPA binary.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use tracing::debug;

use crate::fs_access;

/// Find the OPA binary with optional CLI override
///
/// # Resolution Order
///
/// 1. **CLI override** (if provided) - validated for existence and executability
/// 2. **Bundled OPA** alongside cupcake binary - checked for existence
/// 3. **System PATH** - returned as command name for OS resolution
///
/// # Validation Approach
///
/// Different discovery methods use appropriate validation strategies:
///
/// - **CLI override**: Validated early (user input should fail fast with clear errors)
/// - **Bundled OPA**: Existence check only (known specific location)
/// - **System PATH**: No pre-validation (follows standard Rust practice)
///
/// The PATH fallback returns a command name ("opa" or "opa.exe") that the OS
/// will resolve at execution time. If OPA is not found in PATH, the execution
/// will fail with a helpful error message (see `compile_policies_with_namespace`).
///
/// This approach avoids TOCTOU (time-of-check-time-of-use) issues and matches
/// the pattern used by cargo, rustup, and other Rust ecosystem tools.
///
/// # Returns
///
/// Returns `Ok(PathBuf)` with either an absolute path to a validated OPA binary, or a
/// command name ("opa"/"opa.exe") for PATH resolution. See "Resolution Order" for details.
///
/// # Errors
///
/// Returns `Err` only when CLI override validation fails. PATH resolution errors are
/// deferred to execution time for better error context and to avoid false negatives.
pub fn find_opa_binary(cli_override: Option<PathBuf>) -> Result<PathBuf> {
    // 1. Check CLI override
    if let Some(opa_path) = cli_override {
        validate_opa_override(&opa_path)?;
        debug!("Using CLI --opa-path override: {:?}", opa_path);
        return Ok(opa_path);
    }

    // 2. Check if OPA is bundled alongside cupcake binary
    if let Some(bundled_opa) = bundled_opa_path() {
        debug!("Using bundled OPA at: {:?}", bundled_opa);
        return Ok(bundled_opa);
    }

    // 3. Fall back to system PATH
    //
    // Note: We return the command name without pre-validation. This is intentional and follows
    // standard Rust practice (used by cargo, rustup, git2-rs, etc.) for several reasons:
    //
    // - Avoids TOCTOU issues: Pre-checking with `which` doesn't prevent execution failures
    //   (PATH can change, file can be deleted, permissions can change between check and use)
    // - Better error context: Execution failure (line 244) provides actual error with helpful
    //   message suggesting installation and --opa-path flag
    // - No false negatives: Pre-validation can't check OPA version compatibility, only that
    //   *some* executable exists in PATH
    //
    // If OPA is not found or fails to execute, the error is caught at line 244-246 with a
    // helpful message directing the user to install OPA or use --opa-path.
    debug!("Using OPA from system PATH");
    Ok(if cfg!(windows) {
        PathBuf::from("opa.exe")
    } else {
        PathBuf::from("opa")
    })
}

/// Validate an OPA path given on the command line: it must exist, be a file and,
/// on Unix-like systems, be executable.
fn validate_opa_override(opa_path: &Path) -> Result<()> {
    if !opa_path.exists() {
        bail!("OPA path does not exist: {}", opa_path.display());
    }

    if !opa_path.is_file() {
        bail!("OPA path must be a file: {}", opa_path.display());
    }

    // Check if file is executable (Unix-like systems only)
    #[cfg(unix)]
    {
        let mode = fs_access::unix_mode(opa_path).context("Failed to read OPA file metadata")?;
        if mode & 0o111 == 0 {
            bail!("OPA path is not executable: {}", opa_path.display());
        }
    }

    Ok(())
}

/// The OPA binary bundled alongside the cupcake binary, if there is one.
fn bundled_opa_path() -> Option<PathBuf> {
    let current_exe = std::env::current_exe().ok()?;
    let exe_dir = current_exe.parent()?;
    let bundled_opa = if cfg!(windows) {
        exe_dir.join("opa.exe")
    } else {
        exe_dir.join("opa")
    };
    bundled_opa.exists().then_some(bundled_opa)
}
