//! Watchdog configuration types
//!
//! Configuration for the Watchdog LLM-as-judge feature.
//!
//! ## Configuration Sources (in precedence order)
//!
//! 1. `.cupcake/watchdog/config.json` - Project-level config
//! 2. `~/.config/cupcake/watchdog/config.json` - Global config
//! 3. Built-in defaults
//!
//! ## Directory Structure
//!
//! ```text
//! .cupcake/watchdog/
//! ├── config.json   # Backend, model, timeout settings
//! ├── system.txt    # Custom system prompt (optional)
//! └── user.txt      # User message template with {{event}} (optional)
//! ```
//!
//! ## Rulebook Syntax
//!
//! In rulebook.yml, use `watchdog: true` or `watchdog: false`:
//!
//! ```yaml
//! watchdog: true   # Enable with directory-based config
//! watchdog: false  # Disable
//! ```

use serde::{Deserialize, Deserializer, Serialize};
use std::path::Path;

/// Wrapper for deserializing watchdog config from either `true` or full object
///
/// Supports both:
/// - `watchdog: true` - shorthand for enabled with defaults
/// - `watchdog: { enabled: true, ... }` - full configuration
#[derive(Debug, Clone)]
pub enum WatchdogConfigInput {
    /// Shorthand: `watchdog: true` or `watchdog: false`
    Enabled(bool),
    /// Full config object
    Full(WatchdogConfig),
}

impl<'de> Deserialize<'de> for WatchdogConfigInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::{self, MapAccess, Visitor};

        struct WatchdogConfigInputVisitor;

        impl<'de> Visitor<'de> for WatchdogConfigInputVisitor {
            type Value = WatchdogConfigInput;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a boolean or watchdog configuration object")
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(WatchdogConfigInput::Enabled(value))
            }

            fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let config =
                    WatchdogConfig::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                Ok(WatchdogConfigInput::Full(config))
            }
        }

        deserializer.deserialize_any(WatchdogConfigInputVisitor)
    }
}

impl From<WatchdogConfigInput> for WatchdogConfig {
    fn from(input: WatchdogConfigInput) -> Self {
        match input {
            WatchdogConfigInput::Enabled(enabled) => {
                if enabled {
                    // `watchdog: true` -> enabled with defaults
                    WatchdogConfig {
                        enabled: true,
                        openrouter: Some(OpenRouterConfig::default()),
                        ..Default::default()
                    }
                } else {
                    // `watchdog: false` -> disabled
                    WatchdogConfig::default()
                }
            }
            WatchdogConfigInput::Full(config) => config,
        }
    }
}

impl Default for WatchdogConfigInput {
    fn default() -> Self {
        WatchdogConfigInput::Enabled(false)
    }
}

/// Top-level Watchdog configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchdogConfig {
    /// Whether Watchdog is enabled
    #[serde(default)]
    pub enabled: bool,

    /// Which backend to use ("openrouter" for now, "claude-code" in future)
    #[serde(default = "default_backend")]
    pub backend: String,

    /// Timeout for LLM calls in seconds
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,

    /// Behavior on error: "allow" (fail-open) or "deny" (fail-closed)
    #[serde(default = "default_on_error")]
    pub on_error: String,

    /// Dry run mode - logs resolved config but skips actual LLM calls
    /// Useful for testing configuration without API costs
    #[serde(default)]
    pub dry_run: bool,

    /// OpenRouter-specific configuration
    #[serde(default)]
    pub openrouter: Option<OpenRouterConfig>,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            backend: default_backend(),
            timeout_seconds: default_timeout(),
            on_error: default_on_error(),
            dry_run: false,
            openrouter: None,
        }
    }
}

fn default_backend() -> String {
    "openrouter".to_string()
}

fn default_timeout() -> u64 {
    10
}

fn default_on_error() -> String {
    "allow".to_string()
}

/// OpenRouter backend configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterConfig {
    /// OpenRouter model ID (e.g., "google/gemini-2.5-flash")
    #[serde(default = "default_model")]
    pub model: String,

    /// Environment variable name containing the API key
    #[serde(default = "default_api_key_env")]
    pub api_key_env: String,

    /// Timeout in seconds (inherited from parent if not set)
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,

    /// Custom system prompt (uses default if not set)
    #[serde(default)]
    pub system_prompt: Option<String>,
}

impl Default for OpenRouterConfig {
    fn default() -> Self {
        Self {
            model: default_model(),
            api_key_env: default_api_key_env(),
            timeout_seconds: default_timeout(),
            system_prompt: None,
        }
    }
}

fn default_model() -> String {
    "google/gemini-2.5-flash".to_string()
}

fn default_api_key_env() -> String {
    "OPENROUTER_API_KEY".to_string()
}

fn default_rules_context_root_path() -> String {
    "../..".to_string()
}

fn default_strict() -> bool {
    true
}

/// Configuration for injecting rules context into prompts
///
/// Allows loading files (like CLAUDE.md, .cursorrules) to provide
/// context to the Watchdog LLM about project-specific rules.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RulesContext {
    /// Root path for resolving file paths, relative to config.json location
    /// Default: "../.." (project root, assuming config is in .cupcake/watchdog/)
    #[serde(default = "default_rules_context_root_path")]
    pub root_path: String,

    /// List of files to load, relative to root_path
    #[serde(default)]
    pub files: Vec<String>,

    /// If true (default), fail initialization when any rules file cannot be loaded.
    /// If false, log a warning and continue with available files.
    #[serde(default = "default_strict")]
    pub strict: bool,
}

impl WatchdogConfig {
    /// Check if watchdog is properly configured and can be used
    pub fn is_usable(&self) -> bool {
        if !self.enabled {
            return false;
        }

        // When using directory-based config, openrouter config is auto-created
        matches!(self.backend.as_str(), "openrouter")
    }

    /// Get the effective OpenRouter config, applying defaults
    pub fn effective_openrouter_config(&self) -> OpenRouterConfig {
        let mut config = self.openrouter.clone().unwrap_or_default();
        // Inherit timeout from parent if not explicitly set
        if config.timeout_seconds == default_timeout() {
            config.timeout_seconds = self.timeout_seconds;
        }
        config
    }

    /// Check if errors should allow actions to proceed (fail-open behavior)
    pub fn allows_on_error(&self) -> bool {
        self.on_error == "allow"
    }

    /// Load configuration from watchdog directories
    ///
    /// Takes optional project and global watchdog directory paths.
    /// Tries project config first, then global, then uses defaults.
    pub fn load_from_directory(
        project_watchdog_dir: Option<&Path>,
        global_watchdog_dir: Option<&Path>,
    ) -> Self {
        // Try project config first, then global
        let dir_config = project_watchdog_dir
            .and_then(WatchdogDirConfig::load_from_dir)
            .or_else(|| global_watchdog_dir.and_then(WatchdogDirConfig::load_from_dir));

        if let Some(dir_config) = dir_config {
            dir_config.into_watchdog_config()
        } else {
            // No directory config found, return enabled default
            Self {
                enabled: true,
                openrouter: Some(OpenRouterConfig::default()),
                ..Default::default()
            }
        }
    }
}

mod directory;
#[cfg(test)]
mod tests;

pub use directory::*;
