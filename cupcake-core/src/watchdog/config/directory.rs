//! The flattened `.cupcake/watchdog/config.json` file format and rules context loading.

use super::*;

/// Configuration loaded from `.cupcake/watchdog/config.json`
///
/// This is a flattened structure (no nested `openrouter:` key) for simplicity.
/// Note: While `WatchdogConfig` includes a `dry_run` field for programmatic use,
/// it is intentionally excluded here—`dry_run` cannot be set via config files
/// and should only be set from CLI flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchdogDirConfig {
    /// Which backend to use
    #[serde(default = "default_backend")]
    pub backend: String,

    /// Model ID for the backend
    #[serde(default = "default_model")]
    pub model: String,

    /// Timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,

    /// Behavior on error: "allow" or "deny"
    #[serde(default = "default_on_error")]
    pub on_error: String,

    /// Environment variable name for API key
    #[serde(default = "default_api_key_env")]
    pub api_key_env: String,

    /// Rules context configuration for injecting file contents into prompts
    /// Uses camelCase in JSON: `rulesContext`
    #[serde(default, rename = "rulesContext")]
    pub rules_context: Option<RulesContext>,
}

impl Default for WatchdogDirConfig {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            model: default_model(),
            timeout_seconds: default_timeout(),
            on_error: default_on_error(),
            api_key_env: default_api_key_env(),
            rules_context: None,
        }
    }
}

impl RulesContext {
    /// Load the contents of all configured files
    ///
    /// `config_dir` is the directory containing config.json (e.g., `.cupcake/watchdog/`)
    /// Files are resolved as: config_dir / root_path / file
    ///
    /// Returns an error if `strict` is true and any file fails to load.
    /// If `strict` is false, logs warnings for missing files and continues.
    pub fn load_files(&self, config_dir: &Path) -> Result<String, std::io::Error> {
        if self.files.is_empty() {
            return Ok(String::new());
        }

        let root = config_dir.join(&self.root_path);
        let mut contents = Vec::new();

        for file in &self.files {
            let file_path = root.join(file);
            match std::fs::read_to_string(&file_path) {
                Ok(content) => {
                    tracing::debug!("Loaded rules context file: {}", file_path.display());
                    contents.push(format!("=== {file} ===\n{content}"));
                }
                Err(e) => {
                    if self.strict {
                        return Err(std::io::Error::new(
                            e.kind(),
                            format!("Failed to load rules context file '{file}': {e}"),
                        ));
                    }
                    tracing::warn!(
                        "Failed to load rules context file {}: {}",
                        file_path.display(),
                        e
                    );
                }
            }
        }

        Ok(contents.join("\n\n"))
    }
}

impl WatchdogDirConfig {
    /// Load config from a directory's config.json
    pub fn load_from_dir(dir: &Path) -> Option<Self> {
        let config_path = dir.join("config.json");
        if !config_path.exists() {
            // No config.json, but directory exists - use defaults
            if dir.exists() {
                return Some(Self::default());
            }
            return None;
        }

        match std::fs::read_to_string(&config_path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(config) => Some(config),
                Err(e) => {
                    tracing::warn!("Failed to parse {}: {}", config_path.display(), e);
                    Some(Self::default())
                }
            },
            Err(e) => {
                tracing::warn!("Failed to read {}: {}", config_path.display(), e);
                None
            }
        }
    }

    /// Convert to full WatchdogConfig
    ///
    /// Note: `dry_run` is set to false here - it should be set from CLI flag.
    pub fn into_watchdog_config(self) -> WatchdogConfig {
        WatchdogConfig {
            enabled: true,
            backend: self.backend,
            timeout_seconds: self.timeout_seconds,
            on_error: self.on_error,
            dry_run: false, // Set from CLI flag, not file config
            openrouter: Some(OpenRouterConfig {
                model: self.model,
                api_key_env: self.api_key_env,
                timeout_seconds: self.timeout_seconds,
                system_prompt: None, // Loaded separately from system.txt
            }),
        }
    }
}
