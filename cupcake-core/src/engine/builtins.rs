//! Builtin abstractions - Higher-level policy patterns
//!
//! Provides configuration structures and activation logic for the 5 builtin
//! abstractions that simplify common security patterns without writing Rego.

use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info};

use super::rulebook::SignalConfig;

mod config;
mod signals;
#[cfg(test)]
mod tests;
mod validation;

pub use config::*;

/// Check if a builtin policy should be loaded
pub fn should_load_builtin_policy(policy_path: &Path, enabled_builtins: &[String]) -> bool {
    // Check if this is a builtin policy
    if let Some(parent) = policy_path.parent() {
        if parent.file_name() == Some(std::ffi::OsStr::new("builtins")) {
            // Extract policy name from filename
            if let Some(stem) = policy_path.file_stem() {
                let policy_name = stem.to_string_lossy();
                let should_load = enabled_builtins.contains(&policy_name.to_string());

                if should_load {
                    debug!("Loading enabled builtin policy: {}", policy_name);
                } else {
                    debug!("Skipping disabled builtin policy: {}", policy_name);
                }

                return should_load;
            }
        }
    }

    // Not a builtin policy, always load
    true
}
