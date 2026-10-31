//! Merging Cupcake hooks into existing harness settings.

use super::*;

/// Merge hooks into existing settings without duplicates
pub(super) fn merge_hooks(existing: &mut Value, new: Value) -> Result<()> {
    // Ensure existing is an object
    if !existing.is_object() {
        *existing = json!({});
    }

    // Get or create hooks object
    let hooks = existing
        .as_object_mut()
        .ok_or_else(|| anyhow!("Invalid settings format"))?
        .entry("hooks")
        .or_insert_with(|| json!({}));

    // Ensure hooks is an object
    if !hooks.is_object() {
        *hooks = json!({});
    }

    let new_hooks = new["hooks"]
        .as_object()
        .ok_or_else(|| anyhow!("Invalid hooks format"))?;

    // For each event type in new hooks
    for (event_name, new_matchers) in new_hooks {
        let event_array = hooks
            .as_object_mut()
            .unwrap()
            .entry(event_name)
            .or_insert_with(|| json!([]));

        // Ensure it's an array
        if !event_array.is_array() {
            *event_array = json!([]);
        }

        let event_array = event_array
            .as_array_mut()
            .ok_or_else(|| anyhow!("Invalid event array"))?;

        // Check if this exact configuration already exists
        if let Some(new_matcher_array) = new_matchers.as_array() {
            for new_matcher in new_matcher_array {
                if !contains_matcher(event_array, new_matcher) {
                    event_array.push(new_matcher.clone());
                }
            }
        }
    }

    Ok(())
}

/// Check if a matcher configuration already exists in the array
fn contains_matcher(array: &[Value], matcher: &Value) -> bool {
    array.iter().any(|existing| {
        // Check if matcher patterns are the same
        let same_matcher = existing.get("matcher") == matcher.get("matcher");

        // Check if hook commands are the same
        let existing_hooks = existing.get("hooks").and_then(|h| h.as_array());
        let new_hooks = matcher.get("hooks").and_then(|h| h.as_array());

        if let (Some(existing_hooks), Some(new_hooks)) = (existing_hooks, new_hooks) {
            // Check if any new hook command already exists
            let has_duplicate = new_hooks.iter().any(|new_hook| {
                existing_hooks.iter().any(|existing_hook| {
                    // Compare commands to detect duplicates
                    existing_hook.get("command") == new_hook.get("command")
                })
            });

            same_matcher && has_duplicate
        } else {
            false
        }
    })
}
