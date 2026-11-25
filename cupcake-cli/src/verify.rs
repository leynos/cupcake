//! The `verify` command: report the global and project configuration, the
//! routing maps and the WASM compilation status of the engine.

use anyhow::Result;
use std::path::{Path, PathBuf};
use tracing::{error, info};

use cupcake_core::{engine, fs_access};

/// Print whether the global configuration exists, and how many policies it holds.
fn print_global_config_status() -> Result<()> {
    use cupcake_core::engine::global_config::GlobalPaths;

    println!("\n=== Global Configuration ===");
    match GlobalPaths::discover()? {
        Some(global_paths) if global_paths.is_initialized() => {
            println!("✅ Global config found at: {:?}", global_paths.root);

            // Count global policies
            if let Ok(entries) = fs_access::read_dir(&global_paths.policies) {
                let policy_count = entries
                    .iter()
                    .filter(|path| {
                        path.extension()
                            .and_then(|s| s.to_str())
                            .map(|s| s == "rego")
                            .unwrap_or(false)
                    })
                    .count();
                println!("   Policies: {policy_count} global policies");
            }
        }
        Some(global_paths) => {
            println!(
                "❌ Global config not initialized (location would be: {:?})",
                global_paths.root
            );
            println!("   Run 'cupcake init --global' to initialize");
        }
        None => {
            println!("❌ No global config location available");
            println!("   Run 'cupcake init --global' to initialize");
            println!("   Or use --global-config <PATH> to specify a custom location");
        }
    }
    Ok(())
}

/// Print a routing map as `key -> n policies` with one line per policy package.
fn print_routing_map(map: &std::collections::HashMap<String, Vec<engine::PolicyUnit>>) {
    for (key, policies) in map {
        println!("  {} -> {} policies", key, policies.len());
        for policy in policies {
            println!("    - {}", policy.package_name);
        }
    }
}

/// Print the compiled WASM size of the project and global policy modules.
fn print_wasm_status(engine: &engine::Engine) {
    println!("\n=== WASM Compilation ===");
    if let Some(wasm) = engine.wasm_module() {
        println!("  Project WASM: {} bytes ✅", wasm.len());
    } else {
        // This should never happen now - engine initialization would have failed
        println!("  Project WASM: MISSING ❌");
    }

    if let Some(global_wasm) = engine.global_wasm_module() {
        println!("  Global WASM:  {} bytes ✅", global_wasm.len());
    } else {
        println!("  Global WASM:  Not compiled (no global policies or only system policies)");
    }
}

/// Build the engine for `verify`, or print set-up guidance and exit with status 1.
async fn verify_engine_or_exit(
    policy_dir: &Path,
    harness: cupcake_core::harness::types::HarnessType,
) -> engine::Engine {
    match engine::Engine::new(policy_dir, harness).await {
        Ok(e) => {
            println!("✅ Engine initialized successfully");
            e
        }
        Err(e) => {
            error!("Fatal: Cupcake engine failed to initialize: {:#}", e);
            eprintln!("\n❌ Error: Could not start the Cupcake engine.");
            eprintln!(
                "   Please ensure the OPA CLI is installed and accessible in your system's PATH."
            );
            eprintln!("   You can download it from: https://www.openpolicyagent.org/docs/latest/#running-opa");
            std::process::exit(1);
        }
    }
}

/// Verify the engine configuration for a harness and print a report.
pub(crate) async fn verify_command(
    harness: cupcake_core::harness::types::HarnessType,
    policy_dir: PathBuf,
) -> Result<()> {
    info!("Verifying Cupcake engine configuration...");
    info!("Harness type: {:?}", harness);
    info!("Policy directory: {:?}", policy_dir);

    print_global_config_status()?;

    // Initialize the engine - MUST succeed or we exit
    println!("\n=== Project Configuration ===");
    let engine = verify_engine_or_exit(&policy_dir, harness).await;

    // Display routing maps
    println!("\n=== Project Routing Map ===");
    print_routing_map(engine.routing_map());

    // Display global routing map if it exists
    if !engine.global_routing_map().is_empty() {
        println!("\n=== Global Routing Map ===");
        print_routing_map(engine.global_routing_map());
    }

    print_wasm_status(&engine);

    println!("\n✅ Verification complete!");
    Ok(())
}
