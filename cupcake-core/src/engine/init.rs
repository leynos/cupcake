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
        self.initialize_watchdog();

        // Step 1: Scan for .rego files in harness-specific directory with builtin filtering
        let policy_files = self.scan_project_policy_files().await?;

        // Step 1b: Scan system directory at cupcake root for shared system entrypoint
        let system_files = self.scan_system_policy_files().await;

        // Step 2: Parse selectors and build policy units from harness policies
        self.parse_policy_files(policy_files, "policy").await;

        // Step 2b: Parse system policies
        self.parse_policy_files(system_files, "system policy").await;

        if self.policies.is_empty() {
            warn!("No valid policies found in directory");
            return Ok(());
        }

        // Check for non-system policies (actual user rules, not just infrastructure)
        if !self.has_user_policies() {
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

        self.compile_project_wasm().await?;
        self.dump_routing_diagnostics_if_requested();

        info!("Engine initialization complete");
        Ok(())
    }

    /// Scan the harness-specific policies directory, filtering builtins by the project rulebook.
    async fn scan_project_policy_files(&self) -> Result<Vec<std::path::PathBuf>> {
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
        let harness_subdir = self.harness_subdir();
        let harness_policies_dir = self.paths.policies.join(harness_subdir);
        info!(
            "Scanning harness-specific policies for: {:?} at {:?}",
            self.config.harness, harness_policies_dir
        );

        let policy_files =
            scanner::scan_policies_with_filter(&harness_policies_dir, &enabled_builtins).await?;
        info!(
            "Found {} policy files in {} harness directory",
            policy_files.len(),
            harness_subdir
        );
        Ok(policy_files)
    }

    /// Compile the unified project WASM module and start its runtime (steps 4 and 5).
    async fn compile_project_wasm(&mut self) -> Result<()> {
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
        Ok(())
    }

    /// Step 6: dump routing diagnostics when the CLI debug flag is set.
    ///
    /// This runs after ALL initialization (including global) is complete. A failure only warns.
    fn dump_routing_diagnostics_if_requested(&self) {
        if !self.config.debug_routing {
            return;
        }
        if let Err(e) = self.dump_routing_diagnostics() {
            // Don't fail initialization, just warn
            warn!("Failed to dump routing diagnostics: {}", e);
        }
    }

    /// Initialize global configuration (policies, rulebook, WASM)
    async fn initialize_global(&mut self) -> Result<()> {
        info!("Initializing global configuration...");

        // Verify we have global paths
        let global_policies_path = self
            .paths
            .global_policies
            .clone()
            .context("Global policies path not set")?;
        let global_rulebook_path = self
            .paths
            .global_rulebook
            .clone()
            .context("Global rulebook path not set")?;

        self.load_global_rulebook(&global_rulebook_path).await?;

        // Get global enabled builtins
        let global_enabled_builtins = self
            .global_rulebook
            .as_ref()
            .map(|g| g.builtins.enabled_builtins())
            .unwrap_or_default();

        // Determine harness-specific global policies subdirectory
        let harness_subdir = self.harness_subdir();
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

            self.parse_global_policies(global_policy_files).await;
            self.compile_global_wasm_if_needed().await?;
        }

        info!("Global configuration initialization complete");
        Ok(())
    }

    /// Load the global rulebook when its file exists.
    async fn load_global_rulebook(&mut self, global_rulebook_path: &Path) -> Result<()> {
        if !global_rulebook_path.exists() {
            return Ok(());
        }
        self.global_rulebook = Some(
            rulebook::Rulebook::load_with_conventions(
                global_rulebook_path,
                self.paths
                    .global_signals
                    .as_ref()
                    .context("Global signals path not set")?,
            )
            .await?,
        );
        info!("Global rulebook loaded");
        Ok(())
    }

    /// Parse each global policy file into `global_policies`, logging and skipping failures.
    async fn parse_global_policies(&mut self, global_policy_files: Vec<std::path::PathBuf>) {
        for path in global_policy_files {
            match self.parse_policy(&path).await {
                Ok(mut unit) => {
                    move_to_global_namespace(&mut unit);
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
    }

    /// Build the global routing map and compile the global WASM module when user policies exist.
    async fn compile_global_wasm_if_needed(&mut self) -> Result<()> {
        if self.global_policies.is_empty() {
            return Ok(());
        }

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

        if non_system_count == 0 {
            info!("Only system policies found in global config - skipping global WASM compilation");
            return Ok(());
        }

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
        Ok(())
    }
}

/// Rewrite a policy's package name into the global namespace unless it is already there.
fn move_to_global_namespace(unit: &mut PolicyUnit) {
    if unit.package_name.starts_with("cupcake.global.") {
        return;
    }
    // Replace "cupcake.policies" with "cupcake.global.policies"
    unit.package_name = unit
        .package_name
        .replace("cupcake.policies", "cupcake.global.policies")
        .replace("cupcake.system", "cupcake.global.system");
}

#[cfg(test)]
mod tests {
    //! Tests for the global-namespace rewrite used when parsing global policies.

    use super::*;

    fn unit(package_name: &str) -> PolicyUnit {
        PolicyUnit {
            path: std::path::PathBuf::from("policy.rego"),
            package_name: package_name.to_string(),
            routing: Default::default(),
            metadata: None,
        }
    }

    #[test]
    fn move_to_global_namespace_rewrites_policy_and_system_packages() {
        for (before, after) in [
            ("cupcake.policies.custom", "cupcake.global.policies.custom"),
            (
                "cupcake.policies.builtins.git_guard",
                "cupcake.global.policies.builtins.git_guard",
            ),
            ("cupcake.system", "cupcake.global.system"),
        ] {
            let mut policy = unit(before);
            move_to_global_namespace(&mut policy);
            assert_eq!(policy.package_name, after);
        }
    }

    #[test]
    fn move_to_global_namespace_leaves_global_packages_unchanged() {
        let mut policy = unit("cupcake.global.policies.custom");
        move_to_global_namespace(&mut policy);
        assert_eq!(policy.package_name, "cupcake.global.policies.custom");
    }
}
