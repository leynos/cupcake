//! The Cupcake Engine - Core orchestration module.
//!
//! Provides metadata-driven policy discovery, O(1) routing, WASM evaluation,
//! and decision synthesis.

use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;
use tracing::{debug, error, info, instrument, trace, warn};

use crate::debug::SignalTelemetry;
use crate::telemetry::TelemetryContext;

// Core engine modules - discovery and compilation
pub mod compiler;
pub mod config;
pub mod executor;
pub mod metadata;
pub mod scanner;

// Routing system
pub mod routing;
pub mod routing_debug;

// Policy evaluation
pub mod decision;
pub mod synthesis;
pub mod wasm_runtime;

// Configuration and extensions
pub mod builtins;
pub mod global_config;
pub mod rulebook;

// Diagnostics and debugging
pub mod trace;

// Re-export types for public API
pub use config::{EngineConfig, ProjectPaths, SHELL_COMMAND};
pub use metadata::{PolicyMetadata, PolicyUnit, RoutingDirective};
pub use rulebook::{TelemetryConfig, TelemetryFormat};

// Engine implementation, split by concern
mod evaluation;
mod evaluation_global;
mod init;
mod routing_build;

/// The main Engine struct - a black box with simple public API
pub struct Engine {
    /// Project paths following .cupcake/ convention
    paths: ProjectPaths,

    /// Engine configuration from CLI flags
    config: EngineConfig,

    /// In-memory routing map: event criteria -> policy packages
    routing_map: HashMap<String, Vec<PolicyUnit>>,

    /// Compiled WASM module (stored as bytes)
    wasm_module: Option<Vec<u8>>,

    /// WASM runtime instance
    wasm_runtime: Option<wasm_runtime::WasmRuntime>,

    /// List of all discovered policies
    policies: Vec<PolicyUnit>,

    /// Optional rulebook for signals
    rulebook: Option<rulebook::Rulebook>,

    // Global configuration support (optional - may not exist)
    /// Global policies routing map
    global_routing_map: HashMap<String, Vec<PolicyUnit>>,

    /// Global WASM module (compiled separately)
    global_wasm_module: Option<Vec<u8>>,

    /// Global WASM runtime instance
    global_wasm_runtime: Option<wasm_runtime::WasmRuntime>,

    /// List of global policies
    global_policies: Vec<PolicyUnit>,

    /// Optional global rulebook
    global_rulebook: Option<rulebook::Rulebook>,

    /// Watchdog LLM-as-judge instance (optional)
    watchdog: Option<crate::watchdog::Watchdog>,
}

impl Engine {
    /// Create a new engine instance with the given project path and harness type
    /// Accepts either project root or .cupcake/ directory
    /// For CLI flag overrides, use `new_with_config()`
    pub async fn new(
        project_path: impl AsRef<Path>,
        harness: crate::harness::types::HarnessType,
    ) -> Result<Self> {
        Self::new_with_config(project_path, EngineConfig::new(harness)).await
    }

    /// Create a new engine instance with custom configuration
    /// Accepts either project root or .cupcake/ directory
    /// This is the primary API for CLI integration with flag overrides
    pub async fn new_with_config(
        project_path: impl AsRef<Path>,
        config: EngineConfig,
    ) -> Result<Self> {
        let paths = ProjectPaths::resolve_with_config(project_path, config.global_config.clone())?;

        info!("Initializing Cupcake Engine");
        info!("Project root: {:?}", paths.root);
        info!("Policies directory: {:?}", paths.policies);

        if config.wasm_max_memory.is_some() {
            info!(
                "WASM max memory override: {} bytes",
                config.wasm_max_memory.unwrap()
            );
        }
        if config.opa_path.is_some() {
            info!("OPA path override: {:?}", config.opa_path);
        }
        if config.global_config.is_some() {
            info!("Global config override: {:?}", config.global_config);
        }
        if config.debug_routing {
            info!("Routing debug output enabled");
        }

        // Create engine instance with both project and global support
        let mut engine = Self {
            paths,
            config,
            routing_map: HashMap::new(),
            wasm_module: None,
            wasm_runtime: None,
            policies: Vec::new(),
            rulebook: None,
            // Initialize global fields (will be populated if global config exists)
            global_routing_map: HashMap::new(),
            global_wasm_module: None,
            global_wasm_runtime: None,
            global_policies: Vec::new(),
            global_rulebook: None,
            // Watchdog initialized later from rulebook config
            watchdog: None,
        };

        // Initialize the engine (scan, parse, compile)
        engine.initialize().await?;

        Ok(engine)
    }

    /// Get the routing map (for verification/testing)
    pub fn routing_map(&self) -> &HashMap<String, Vec<PolicyUnit>> {
        &self.routing_map
    }

    /// Get the compiled WASM module (for verification/testing)
    pub fn wasm_module(&self) -> Option<&[u8]> {
        self.wasm_module.as_deref()
    }

    /// Get the global routing map (for verification/testing)
    pub fn global_routing_map(&self) -> &HashMap<String, Vec<PolicyUnit>> {
        &self.global_routing_map
    }

    /// Get the compiled global WASM module (for verification/testing)
    pub fn global_wasm_module(&self) -> Option<&[u8]> {
        self.global_wasm_module.as_deref()
    }

    /// Get the telemetry configuration from the rulebook
    pub fn telemetry_config(&self) -> Option<&TelemetryConfig> {
        self.rulebook.as_ref().map(|rb| &rb.telemetry)
    }
}
