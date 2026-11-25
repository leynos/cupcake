//! Named steps of engine initialisation, called from `Engine::initialize`.

use super::*;
use std::path::PathBuf;

impl Engine {
    /// Initialize the watchdog when the project rulebook enables it.
    ///
    /// The watchdog uses directory-based configuration from `.cupcake/watchdog/`
    /// with a fallback to `~/.config/cupcake/watchdog/` for global settings. A
    /// failure to start it is logged and initialization continues without it.
    pub(super) fn initialize_watchdog(&mut self) {
        if !self.rulebook.as_ref().is_some_and(|r| r.watchdog.enabled) {
            return;
        }

        // Get watchdog directories for config/prompt loading
        let project_watchdog_dir = self.paths.project_watchdog_dir();
        let global_watchdog_dir = self.paths.global_watchdog_dir();

        debug!(
            "Watchdog directories: project={:?}, global={:?}",
            project_watchdog_dir, global_watchdog_dir
        );

        // Use from_directories() for full directory-based config loading
        match crate::watchdog::Watchdog::from_directories(
            project_watchdog_dir.as_deref(),
            global_watchdog_dir.as_deref(),
        ) {
            Ok(watchdog) if watchdog.is_enabled() => {
                info!("Watchdog initialized and ready");
                self.watchdog = Some(watchdog);
            }
            Ok(_) => {
                warn!("Watchdog enabled in config but failed to initialize backend");
            }
            Err(e) => {
                warn!(
                    "Failed to initialize Watchdog: {e}. Continuing without it. \
                    Check that your API key environment variable is set and the watchdog configuration is valid."
                );
            }
        }
    }

    /// The policies subdirectory for the configured harness.
    pub(super) fn harness_subdir(&self) -> &'static str {
        match self.config.harness {
            crate::harness::types::HarnessType::ClaudeCode => "claude",
            crate::harness::types::HarnessType::Cursor => "cursor",
            crate::harness::types::HarnessType::Factory => "factory",
            crate::harness::types::HarnessType::OpenCode => "opencode",
        }
    }

    /// Scan the shared `system` directory at the cupcake root, when present.
    ///
    /// A scan failure is logged and treated as no system policies.
    pub(super) async fn scan_system_policy_files(&self) -> Vec<PathBuf> {
        let system_dir = self.paths.cupcake_dir.join("system");
        let system_files = if system_dir.exists() && system_dir.is_dir() {
            info!("Scanning system directory: {:?}", system_dir);
            match scanner::scan_policies(&system_dir).await {
                Ok(files) => files,
                Err(e) => {
                    warn!("Failed to scan system directory: {}", e);
                    Vec::new()
                }
            }
        } else {
            debug!("No system directory found at {:?}", system_dir);
            Vec::new()
        };
        info!("Found {} system policy files", system_files.len());
        system_files
    }

    /// Parse each policy file into a unit, skipping (and logging) bad ones.
    ///
    /// `label` names the kind of policy in log lines, such as `system policy`.
    pub(super) async fn parse_policy_files(&mut self, files: Vec<PathBuf>, label: &str) {
        for path in files {
            match self.parse_policy(&path).await {
                Ok(unit) => {
                    info!(
                        "Successfully parsed {label}: {} from {:?}",
                        unit.package_name, path
                    );
                    self.policies.push(unit);
                }
                Err(e) => {
                    // Fail loudly but don't crash - log and skip bad policies
                    error!("Failed to parse {label} at {:?}: {}", path, e);
                }
            }
        }
    }

    /// Whether any parsed policy is an actual user rule.
    ///
    /// System policies (`cupcake.system.*`, `cupcake.global.system.*`) are
    /// entrypoints and helpers that aggregate decisions; they hold no rules.
    pub(super) fn has_user_policies(&self) -> bool {
        self.policies.iter().any(|p| {
            !p.package_name.starts_with("cupcake.system")
                && !p.package_name.starts_with("cupcake.global.system")
        })
    }
}
