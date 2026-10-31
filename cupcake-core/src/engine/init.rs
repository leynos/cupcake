//! Engine initialisation: scanning, parsing and compiling project and global policies.

use super::*;

impl Engine {
    /// Initialize the engine by scanning, parsing, and compiling policies
    pub(super) async fn initialize(&mut self) -> Result<()> {
        info!("Starting engine initialization...");

        // Step 0A: Initialize global configuration first (if it exists)
        if self.paths.global_root.is_some() {
            info!("Global configuration detected - initializing global policies first");
            self.initialize_global().await?;
        }

        // Step 0B: Load project rulebook to get builtin configuration
        self.rulebook = Some(
            rulebook::Rulebook::load_with_conventions(&self.paths.rulebook, &self.paths.signals)
                .await?,
        );
        info!("Project rulebook loaded with convention-based discovery");

        // Step 0C: Initialize Watchdog if enabled in rulebook
        // Watchdog uses directory-based configuration from .cupcake/watchdog/
        // with fallback to ~/.config/cupcake/watchdog/ for global settings
        if let Some(ref rulebook) = self.rulebook {
            if rulebook.watchdog.enabled {
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
                    Ok(watchdog) => {
                        if watchdog.is_enabled() {
                            info!("Watchdog initialized and ready");
                            self.watchdog = Some(watchdog);
                        } else {
                            warn!("Watchdog enabled in config but failed to initialize backend");
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Failed to initialize Watchdog: {e}. Continuing without it. \
                            Check that your API key environment variable is set and the watchdog configuration is valid."
                        );
                    }
                }
            }
        }

        // Get list of enabled builtins for filtering
        let enabled_builtins = self
            .rulebook
            .as_ref()
            .map(|g| g.builtins.enabled_builtins())
            .unwrap_or_default();

        if !enabled_builtins.is_empty() {
            info!("Enabled builtins: {:?}", enabled_builtins);
        }

        // Determine harness-specific policies subdirectory
        let harness_subdir = match self.config.harness {
            crate::harness::types::HarnessType::ClaudeCode => "claude",
            crate::harness::types::HarnessType::Cursor => "cursor",
            crate::harness::types::HarnessType::Factory => "factory",
            crate::harness::types::HarnessType::OpenCode => "opencode",
        };
        let harness_policies_dir = self.paths.policies.join(harness_subdir);
        info!(
            "Scanning harness-specific policies for: {:?} at {:?}",
            self.config.harness, harness_policies_dir
        );

        // Step 1: Scan for .rego files in harness-specific directory with builtin filtering
        let policy_files =
            scanner::scan_policies_with_filter(&harness_policies_dir, &enabled_builtins).await?;
        info!(
            "Found {} policy files in {} harness directory",
            policy_files.len(),
            harness_subdir
        );

        // Step 1b: Scan system directory at cupcake root for shared system entrypoint
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

        // Step 2: Parse selectors and build policy units from harness policies
        for path in policy_files {
            match self.parse_policy(&path).await {
                Ok(unit) => {
                    info!(
                        "Successfully parsed policy: {} from {:?}",
                        unit.package_name, path
                    );
                    self.policies.push(unit);
                }
                Err(e) => {
                    // Fail loudly but don't crash - log and skip bad policies
                    error!("Failed to parse policy at {:?}: {}", path, e);
                }
            }
        }

        // Step 2b: Parse system policies
        for path in system_files {
            match self.parse_policy(&path).await {
                Ok(unit) => {
                    info!(
                        "Successfully parsed system policy: {} from {:?}",
                        unit.package_name, path
                    );
                    self.policies.push(unit);
                }
                Err(e) => {
                    error!("Failed to parse system policy at {:?}: {}", path, e);
                }
            }
        }

        if self.policies.is_empty() {
            warn!("No valid policies found in directory");
            return Ok(());
        }

        // Check for non-system policies (actual user rules, not just infrastructure)
        // System policies (cupcake.system.*, cupcake.global.system.*) are entrypoints/helpers
        // that aggregate decisions - they don't contain actual rules themselves.
        let non_system_count = self
            .policies
            .iter()
            .filter(|p| {
                !p.package_name.starts_with("cupcake.system")
                    && !p.package_name.starts_with("cupcake.global.system")
            })
            .count();

        if non_system_count == 0 {
            info!(
                "No user policies found (only system infrastructure) - evaluation will allow all"
            );
            // Build routing map anyway (will be empty, but consistent)
            self.build_routing_map();
            // Skip WASM compilation - no policies to evaluate
            // wasm_runtime remains None, evaluate() will return Allow
            return Ok(());
        }

        // Step 3: Build routing map
        self.build_routing_map();
        info!("Built routing map with {} entries", self.routing_map.len());

        // No entrypoint mapping needed - Hybrid Model uses single aggregation entrypoint

        // Step 4: Compile unified WASM module with OPA path from CLI
        // Pass cupcake_dir for helpers resolution at root level
        let wasm_bytes = compiler::compile_policies_with_namespace(
            &self.policies,
            "cupcake.system",
            self.config.opa_path.clone(),
            Some(&self.paths.cupcake_dir),
        )
        .await?;
        info!(
            "Successfully compiled unified WASM module ({} bytes)",
            wasm_bytes.len()
        );
        self.wasm_module = Some(wasm_bytes.clone());

        // Step 5: Initialize WASM runtime with memory config from CLI
        self.wasm_runtime = Some(wasm_runtime::WasmRuntime::new_with_config(
            &wasm_bytes,
            "cupcake.system",
            self.config.wasm_max_memory,
        )?);
        info!("WASM runtime initialized");

        // Step 6: Dump routing diagnostics if debug mode enabled via CLI flag
        // This happens after ALL initialization (including global) is complete
        if self.config.debug_routing {
            if let Err(e) = self.dump_routing_diagnostics() {
                // Don't fail initialization, just warn
                warn!("Failed to dump routing diagnostics: {}", e);
            }
        }

        info!("Engine initialization complete");
        Ok(())
    }

    /// Initialize global configuration (policies, rulebook, WASM)
    async fn initialize_global(&mut self) -> Result<()> {
        info!("Initializing global configuration...");

        // Verify we have global paths
        let global_policies_path = self
            .paths
            .global_policies
            .as_ref()
            .context("Global policies path not set")?;
        let global_rulebook_path = self
            .paths
            .global_rulebook
            .as_ref()
            .context("Global rulebook path not set")?;

        // Load global rulebook
        if global_rulebook_path.exists() {
            self.global_rulebook = Some(
                rulebook::Rulebook::load_with_conventions(
                    global_rulebook_path,
                    self.paths.global_signals.as_ref().unwrap(),
                )
                .await?,
            );
            info!("Global rulebook loaded");
        }

        // Get global enabled builtins
        let global_enabled_builtins = self
            .global_rulebook
            .as_ref()
            .map(|g| g.builtins.enabled_builtins())
            .unwrap_or_default();

        // Determine harness-specific global policies subdirectory
        let harness_subdir = match self.config.harness {
            crate::harness::types::HarnessType::ClaudeCode => "claude",
            crate::harness::types::HarnessType::Cursor => "cursor",
            crate::harness::types::HarnessType::Factory => "factory",
            crate::harness::types::HarnessType::OpenCode => "opencode",
        };
        let harness_global_policies_dir = global_policies_path.join(harness_subdir);

        // Scan for global policies in harness-specific directory
        if harness_global_policies_dir.exists() {
            info!(
                "Scanning global harness-specific policies for: {:?} at {:?}",
                self.config.harness, harness_global_policies_dir
            );
            let global_policy_files = scanner::scan_policies_with_filter(
                &harness_global_policies_dir,
                &global_enabled_builtins,
            )
            .await?;
            info!(
                "Found {} global policy files in {} harness directory",
                global_policy_files.len(),
                harness_subdir
            );

            // Parse global policies
            for path in global_policy_files {
                match self.parse_policy(&path).await {
                    Ok(mut unit) => {
                        // Transform package name to global namespace
                        if !unit.package_name.starts_with("cupcake.global.") {
                            // Replace "cupcake.policies" with "cupcake.global.policies"
                            unit.package_name = unit
                                .package_name
                                .replace("cupcake.policies", "cupcake.global.policies")
                                .replace("cupcake.system", "cupcake.global.system");
                        }
                        info!(
                            "Successfully parsed global policy: {} from {:?}",
                            unit.package_name, path
                        );
                        self.global_policies.push(unit);
                    }
                    Err(e) => {
                        error!("Failed to parse global policy at {:?}: {}", path, e);
                    }
                }
            }

            if !self.global_policies.is_empty() {
                // Check if we have non-system policies (OPA panics with only system policies)
                info!(
                    "Global policies found: {:?}",
                    self.global_policies
                        .iter()
                        .map(|p| &p.package_name)
                        .collect::<Vec<_>>()
                );
                let non_system_count = self
                    .global_policies
                    .iter()
                    .filter(|p| !p.package_name.ends_with(".system"))
                    .count();

                if non_system_count > 0 {
                    // Build global routing map
                    self.build_global_routing_map();
                    info!(
                        "Built global routing map with {} entries",
                        self.global_routing_map.len()
                    );

                    info!(
                        "Found {} global policies ({} non-system) - compiling global WASM module",
                        self.global_policies.len(),
                        non_system_count
                    );

                    // Compile global policies to WASM with OPA path from CLI
                    // Pass global_root for helpers resolution at root level
                    let global_wasm_bytes = compiler::compile_policies_with_namespace(
                        &self.global_policies,
                        "cupcake.global.system",
                        self.config.opa_path.clone(),
                        self.paths.global_root.as_deref(),
                    )
                    .await?;
                    info!(
                        "Successfully compiled global WASM module ({} bytes)",
                        global_wasm_bytes.len()
                    );
                    self.global_wasm_module = Some(global_wasm_bytes.clone());

                    // Initialize global WASM runtime with global namespace and memory config
                    self.global_wasm_runtime = Some(wasm_runtime::WasmRuntime::new_with_config(
                        &global_wasm_bytes,
                        "cupcake.global.system",
                        self.config.wasm_max_memory,
                    )?);
                    info!("Global WASM runtime initialized with namespace: cupcake.global.system");
                } else {
                    info!("Only system policies found in global config - skipping global WASM compilation");
                }
            }
        }

        info!("Global configuration initialization complete");
        Ok(())
    }
}
