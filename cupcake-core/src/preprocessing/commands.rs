//! Bash command preprocessing: whitespace normalization, script inspection and affected directories.

use super::*;

/// Preprocess Claude Code Bash commands to normalize whitespace and inspect scripts
/// Returns true if whitespace normalization was actually applied
pub(super) fn preprocess_claude_bash_command(input: &mut Value, config: &PreprocessConfig) -> bool {
    let mut applied = false;

    // Navigate to tool_input.command (Claude Code structure)
    if let Some(tool_input) = input.get_mut("tool_input") {
        if let Some(command_value) = tool_input.get_mut("command") {
            if let Some(command) = command_value.as_str() {
                let original = command.to_string();
                let mut normalized = WhitespaceNormalizer::normalize_command(&original);

                // Only update if changed
                if original != normalized {
                    *command_value = Value::String(normalized.clone());
                    applied = true;

                    if config.audit_transformations {
                        debug!(
                            "Normalized Claude Code Bash command: '{}' → '{}'",
                            original, normalized
                        );
                    }
                } else {
                    normalized = original;
                }

                // Check for script execution if enabled
                if config.enable_script_inspection {
                    inspect_and_attach_script(input, &normalized);
                }

                // Extract affected parent directories for destructive commands
                // This enables policies to protect paths that would be affected
                // by operations like `rm -rf /parent/*` where /parent/.cupcake/ exists
                extract_and_attach_affected_directories(input, &normalized);
            }
        }
    }

    applied
}

/// Preprocess Cursor shell commands to normalize whitespace and inspect scripts
/// Returns true if whitespace normalization was actually applied
pub(super) fn preprocess_cursor_shell_command(
    input: &mut Value,
    config: &PreprocessConfig,
) -> bool {
    // Navigate to command (Cursor structure - command is at root level)
    let Some(original) = input
        .get("command")
        .and_then(|command| command.as_str())
        .map(str::to_string)
    else {
        return false;
    };

    let normalized = WhitespaceNormalizer::normalize_command(&original);

    // Only update if changed
    let applied = original != normalized;
    if applied {
        input["command"] = Value::String(normalized.clone());

        if config.audit_transformations {
            debug!(
                "Normalized Cursor shell command: '{}' → '{}'",
                original, normalized
            );
        }
    }

    // Check for script execution if enabled
    if config.enable_script_inspection {
        inspect_and_attach_script(input, &normalized);
    }

    // Extract affected parent directories for destructive commands
    extract_and_attach_affected_directories(input, &normalized);

    applied
}

/// Extract affected parent directories from destructive commands and attach to event
///
/// This enables policies to detect when a command like `rm -rf /parent/*` would
/// affect protected child paths like `/parent/.cupcake/`.
///
/// The extracted paths are canonicalized via SymlinkResolver to prevent bypass
/// attacks where the parent directory is a symlink.
fn extract_and_attach_affected_directories(input: &mut Value, command: &str) {
    let affected_paths = extract_target_paths(command);

    if affected_paths.is_empty() {
        return;
    }

    // Get cwd for path resolution
    let cwd: Option<std::path::PathBuf> = input
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(std::path::PathBuf::from);

    // Canonicalize each path via SymlinkResolver
    let mut canonical_paths: Vec<String> = Vec::new();

    for path in affected_paths {
        if let Some(resolved) = SymlinkResolver::resolve_path(&path, cwd.as_deref()) {
            let resolved_str = resolved.to_string_lossy().to_string();
            debug!("Resolved affected directory: {:?} → {}", path, resolved_str);
            canonical_paths.push(resolved_str);
        } else {
            // Fallback: use the path as-is if we can't resolve
            // (e.g., path doesn't exist yet)
            let path_str = if path.is_absolute() {
                path.to_string_lossy().to_string()
            } else if let Some(ref cwd) = cwd {
                cwd.join(&path).to_string_lossy().to_string()
            } else {
                path.to_string_lossy().to_string()
            };
            trace!(
                "Could not resolve affected directory, using raw: {}",
                path_str
            );
            canonical_paths.push(path_str);
        }
    }

    if !canonical_paths.is_empty() {
        // Attach to event
        if let Some(obj) = input.as_object_mut() {
            obj.insert(
                "affected_parent_directories".to_string(),
                serde_json::Value::Array(
                    canonical_paths
                        .iter()
                        .map(|s| serde_json::Value::String(s.clone()))
                        .collect(),
                ),
            );
            debug!(
                "Attached affected_parent_directories: {:?}",
                canonical_paths
            );
        }
    }
}

/// Helper function to inspect command for script execution and attach script content
fn inspect_and_attach_script(input: &mut Value, command: &str) {
    // Check if the command executes a script
    if let Some(script_path) = ScriptInspector::detect_script_execution(command) {
        trace!("Detected script execution: {:?}", script_path);

        // Get the working directory from the event (if available)
        let cwd = input
            .get("cwd")
            .and_then(|v| v.as_str())
            .map(std::path::Path::new);

        // Try to load the script content
        if let Some(script_content) = ScriptInspector::load_script_content(&script_path, cwd) {
            // Attach the script content to the event
            ScriptInspector::attach_script_to_event(input, &script_path, &script_content);

            debug!(
                "Attached script content from {:?} ({} bytes)",
                script_path,
                script_content.len()
            );
        } else {
            trace!("Could not load script content from {:?}", script_path);
        }
    }
}
