//! Symlink resolution for the file paths of file operations (TOB-4 defence).

use super::*;

/// Helper function to resolve symlinks in file paths and attach metadata
///
/// This implements TOB-4 defense by detecting when file paths are symbolic links
/// and resolving them to their canonical target paths. This prevents bypass attacks
/// where attackers create symlinks to protected directories.
/// Returns true if any file path was processed for symlink resolution.
pub(super) fn resolve_and_attach_symlinks(input: &mut Value, harness: HarnessType) -> bool {
    let mut applied = false;

    // Get the working directory from the event (if available) - shared across all paths
    // Clone the path to avoid borrow checker issues
    let cwd: Option<std::path::PathBuf> = input
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(std::path::PathBuf::from);

    // Extract file path based on tool and harness type
    let file_path_opt = match harness {
        HarnessType::ClaudeCode | HarnessType::Factory => {
            // Claude Code and Factory AI structure: input.tool_input.<field>
            input.get("tool_input").and_then(|tool_input| {
                // Try different field names based on tool type
                // NOTE: Glob 'pattern' field is intentionally excluded - patterns like
                // "src/**/*.rs" are not file paths and should not be canonicalized
                tool_input
                    .get("file_path")
                    .or_else(|| tool_input.get("path"))
                    .or_else(|| tool_input.get("notebook_path"))
            })
        }
        HarnessType::Cursor => {
            // Cursor structure: input.<field> (direct at root)
            input.get("file_path").or_else(|| input.get("path"))
        }
        HarnessType::OpenCode => {
            // OpenCode uses same structure as Claude Code/Factory: input.tool_input.<field>
            input.get("tool_input").and_then(|tool_input| {
                tool_input
                    .get("file_path")
                    .or_else(|| tool_input.get("path"))
                    .or_else(|| tool_input.get("filePath")) // OpenCode may use camelCase
            })
        }
    };

    // Process single file path if present
    if let Some(path_value) = file_path_opt {
        if let Some(path_str) = path_value.as_str() {
            let path_str_owned = path_str.to_string();
            resolve_and_attach_single_path(input, &path_str_owned, cwd.as_deref());
            applied = true;
        }
    }

    // Special handling for MultiEdit tool (Claude Code only)
    if harness == HarnessType::ClaudeCode {
        // First, collect all the file paths to avoid borrow checker issues
        let edit_paths: Vec<String> = input
            .get("tool_input")
            .and_then(|ti| ti.get("edits"))
            .and_then(|e| e.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|edit| {
                        edit.get("file_path")
                            .and_then(|fp| fp.as_str())
                            .map(|s| s.to_string())
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Now process each edit with mutable access
        if !edit_paths.is_empty() {
            if let Some(tool_input) = input.get_mut("tool_input") {
                if let Some(edits) = tool_input.get_mut("edits") {
                    if let Some(edits_array) = edits.as_array_mut() {
                        for (i, edit) in edits_array.iter_mut().enumerate() {
                            if let Some(path_str) = edit_paths.get(i) {
                                trace!("Canonicalizing MultiEdit file_path: {}", path_str);
                                resolve_and_attach_single_path(edit, path_str, cwd.as_deref());
                                applied = true;
                            }
                        }
                    }
                }
            }
        }
    }

    applied
}

/// Resolve and attach a single file path's canonical form
fn resolve_and_attach_single_path(
    target: &mut Value,
    path_str: &str,
    cwd: Option<&std::path::Path>,
) {
    let path = std::path::Path::new(path_str);

    // TOB-4: ALWAYS canonicalize paths (not just symlinks)
    // This provides defense-in-depth against:
    // - Symlink bypass attacks
    // - Relative path tricks (../../.cupcake/)
    // - Path normalization issues
    trace!("Canonicalizing file path: {}", path_str);

    if let Some(resolved_path) = SymlinkResolver::resolve_path(path, cwd) {
        // Check if this was a symlink
        let is_symlink = SymlinkResolver::is_symlink(path);

        // Attach canonical path metadata to the event/edit
        SymlinkResolver::attach_metadata(target, path_str, &resolved_path, is_symlink);

        debug!(
            "Canonicalized path: {} -> {:?} (symlink: {})",
            path_str, resolved_path, is_symlink
        );
    } else {
        // FALLBACK: Path doesn't exist (e.g., Write creating new file)
        // Still provide a resolved path by manually joining CWD + path
        // This ensures policies ALWAYS have resolved_file_path available
        trace!(
            "Could not canonicalize path: {} (file/parent doesn't exist)",
            path_str
        );

        let fallback_path = if path.is_absolute() {
            path.to_path_buf()
        } else if let Some(cwd) = cwd {
            cwd.join(path)
        } else {
            // No CWD provided - use current directory
            std::env::current_dir()
                .ok()
                .map(|c| c.join(path))
                .unwrap_or_else(|| path.to_path_buf())
        };

        // Attach metadata with fallback path (is_symlink = false since we couldn't verify)
        SymlinkResolver::attach_metadata(target, path_str, &fallback_path, false);

        debug!(
            "Using fallback path resolution: {} -> {:?} (non-existent)",
            path_str, fallback_path
        );
    }
}
