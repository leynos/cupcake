//! Migration utilities for upgrading Cupcake project configurations
//!
//! This module handles backwards-compatible migrations when the project
//! structure changes between Cupcake versions.

use anyhow::{Context, Result};
use cupcake_core::fs_access;
use std::path::Path;

/// Migrate legacy helpers/ directory to system/ directory
///
/// Projects created before the helpers consolidation had:
/// - `.cupcake/helpers/commands.rego` with package `cupcake.helpers.commands`
///
/// New projects have:
/// - `.cupcake/system/commands.rego` with package `cupcake.system.commands`
///
/// This migration:
/// 1. Moves .rego files from helpers/ to system/
/// 2. Updates package declarations from `cupcake.helpers.` to `cupcake.system.`
/// 3. Updates all policy imports to use the new paths
/// 4. Removes the empty helpers/ directory
pub fn migrate_helpers_to_system(helpers_dir: &Path, system_dir: &Path) -> Result<()> {
    // Move any .rego files from helpers/ to system/
    for path in fs_access::read_dir(helpers_dir)? {
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "rego" {
                    let file_name = path.file_name().unwrap();
                    let dest = system_dir.join(file_name);

                    // Read the file content and update the package declaration
                    let content = fs_access::read_to_string(&path)?;
                    let updated_content =
                        content.replace("package cupcake.helpers.", "package cupcake.system.");

                    // Write to system/ with updated package
                    fs_access::write(&dest, updated_content).with_context(|| {
                        format!("Failed to migrate {} to system/", path.display())
                    })?;

                    // Remove the old file
                    fs_access::remove_file(&path).with_context(|| {
                        format!("Failed to remove old helpers file {}", path.display())
                    })?;

                    eprintln!("  Migrated: {} -> {}", path.display(), dest.display());
                }
            }
        }
    }

    // Update all policy files to use new import paths
    let policies_dir = helpers_dir.parent().unwrap().join("policies");
    if policies_dir.exists() {
        update_policy_imports(&policies_dir)?;
    }

    // Remove the now-empty helpers directory
    if fs_access::read_dir(helpers_dir)?.is_empty() {
        fs_access::remove_dir(helpers_dir)
            .with_context(|| "Failed to remove empty helpers directory")?;
        eprintln!("  Removed empty helpers/ directory");
    }

    Ok(())
}

/// Update policy files to use new cupcake.system imports instead of cupcake.helpers
fn update_policy_imports(policies_dir: &Path) -> Result<()> {
    let mut files = Vec::new();
    find_rego_files(policies_dir, &mut files)?;

    for file in files {
        let content = fs_access::read_to_string(&file)?;
        if content.contains("data.cupcake.helpers.") {
            let updated = content
                .replace(
                    "import data.cupcake.helpers.",
                    "import data.cupcake.system.",
                )
                .replace("data.cupcake.helpers.", "data.cupcake.system.");
            fs_access::write(&file, updated)
                .with_context(|| format!("Failed to update imports in {}", file.display()))?;
            eprintln!("  Updated imports: {}", file.display());
        }
    }

    Ok(())
}

/// Recursively find all .rego files in a directory
fn find_rego_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) -> Result<()> {
    for path in fs_access::read_dir(dir)? {
        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "rego" {
                    files.push(path);
                }
            }
        } else if path.is_dir() {
            find_rego_files(&path, files)?;
        }
    }

    Ok(())
}
