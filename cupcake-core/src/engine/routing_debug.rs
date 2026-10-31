//! Routing map debug utilities for serializing and visualizing the routing system
//!
//! Provides comprehensive debugging output for the routing map data structure,
//! including JSON serialization, human-readable text format, and graph visualization.
//!
//! Enable via --debug-routing CLI flag.
//! In production builds, can be disabled with --no-default-features to exclude this module.

use anyhow::Result;
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tracing::info;

use super::{Engine, PolicyUnit};

/// Simplified policy info for serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimplifiedPolicyInfo {
    pub package_name: String,
    pub file_path: String,
    pub required_events: Vec<String>,
    pub required_tools: Vec<String>,
    pub required_signals: Vec<String>,
}

impl From<&PolicyUnit> for SimplifiedPolicyInfo {
    fn from(policy: &PolicyUnit) -> Self {
        Self {
            package_name: policy.package_name.clone(),
            file_path: policy.path.to_string_lossy().to_string(),
            required_events: policy.routing.required_events.clone(),
            required_tools: policy.routing.required_tools.clone(),
            required_signals: policy.routing.required_signals.clone(),
        }
    }
}

/// Complete routing map dump for analysis
#[derive(Debug, Serialize, Deserialize)]
pub struct RoutingMapDump {
    pub timestamp: String,
    pub project: RoutingMapSection,
    pub global: RoutingMapSection,
    pub statistics: RoutingStatistics,
}

/// Section of routing map (project or global)
#[derive(Debug, Serialize, Deserialize)]
pub struct RoutingMapSection {
    pub policy_count: usize,
    pub routing_entries: HashMap<String, Vec<SimplifiedPolicyInfo>>,
}

/// Routing statistics for analysis
#[derive(Debug, Serialize, Deserialize)]
pub struct RoutingStatistics {
    pub total_routes: usize,
    pub wildcard_routes: usize,
    pub specific_routes: usize,
    pub events_covered: Vec<String>,
    pub tools_covered: Vec<String>,
    pub average_policies_per_route: f64,
}

impl Engine {
    /// Main entry point for dumping routing diagnostics
    ///
    /// Note: Caller should only call this if debug routing is enabled via CLI flag.
    /// The check is done in main.rs when deciding whether to call this method.
    pub fn dump_routing_diagnostics(&self) -> Result<()> {
        // Create debug directory
        let debug_dir = Path::new(".cupcake/debug/routing");
        crate::fs_access::create_dir_all(debug_dir)?;

        let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S");

        // Write all three formats - they each serve different purposes
        self.write_json_dump(debug_dir, &timestamp)?;
        self.write_text_dump(debug_dir, &timestamp)?;
        self.write_dot_graph(debug_dir, &timestamp)?;

        info!("Routing diagnostics written to .cupcake/debug/routing/");
        info!("  JSON: routing_map_{}.json", timestamp);
        info!("  Text: routing_map_{}.txt", timestamp);
        info!("  Graph: routing_map_{}.dot", timestamp);

        Ok(())
    }

    /// Write JSON dump for programmatic analysis
    fn write_json_dump(&self, debug_dir: &Path, timestamp: &impl std::fmt::Display) -> Result<()> {
        let json_file = debug_dir.join(format!("routing_map_{timestamp}.json"));

        // Convert routing maps to simplified format
        let project_routing = self.convert_routing_map(&self.routing_map);
        let global_routing = self.convert_routing_map(&self.global_routing_map);

        let routing_dump = RoutingMapDump {
            timestamp: timestamp.to_string(),
            project: RoutingMapSection {
                policy_count: self.policies.len(),
                routing_entries: project_routing,
            },
            global: RoutingMapSection {
                policy_count: self.global_policies.len(),
                routing_entries: global_routing,
            },
            statistics: self.compute_routing_statistics(),
        };

        let json = serde_json::to_string_pretty(&routing_dump)?;
        crate::fs_access::write(json_file, json)?;

        Ok(())
    }

    /// Convert routing map to simplified format
    fn convert_routing_map(
        &self,
        map: &HashMap<String, Vec<PolicyUnit>>,
    ) -> HashMap<String, Vec<SimplifiedPolicyInfo>> {
        map.iter()
            .map(|(key, policies)| {
                let simplified: Vec<SimplifiedPolicyInfo> =
                    policies.iter().map(SimplifiedPolicyInfo::from).collect();
                (key.clone(), simplified)
            })
            .collect()
    }

    /// Write human-readable text dump
    fn write_text_dump(&self, debug_dir: &Path, timestamp: &impl std::fmt::Display) -> Result<()> {
        let txt_file = debug_dir.join(format!("routing_map_{timestamp}.txt"));
        let readable = self.format_routing_map_human_readable();
        crate::fs_access::write(txt_file, readable)?;
        Ok(())
    }

    /// Format routing map as human-readable text
    fn format_routing_map_human_readable(&self) -> String {
        let mut output = String::new();

        output.push_str("==================================================\n");
        output.push_str("           CUPCAKE ROUTING MAP DUMP              \n");
        output.push_str("==================================================\n\n");

        output.push_str(&format!(
            "Generated: {}\n",
            Local::now().format("%Y-%m-%d %H:%M:%S")
        ));
        output.push_str(&format!(
            "Working Directory: {:?}\n",
            std::env::current_dir().ok()
        ));
        output.push('\n');

        // Statistics
        output.push_str("STATISTICS:\n");
        output.push_str(&format!("  Project Policies: {}\n", self.policies.len()));
        output.push_str(&format!("  Project Routes: {}\n", self.routing_map.len()));
        output.push_str(&format!(
            "  Global Policies: {}\n",
            self.global_policies.len()
        ));
        output.push_str(&format!(
            "  Global Routes: {}\n",
            self.global_routing_map.len()
        ));
        output.push('\n');

        // Project routing map
        if !self.routing_map.is_empty() {
            output.push_str("PROJECT ROUTING MAP:\n");
            output.push_str("--------------------\n\n");
            output.push_str(&self.format_routing_section(&self.routing_map));
        }

        // Global routing map
        if !self.global_routing_map.is_empty() {
            output.push_str("GLOBAL ROUTING MAP:\n");
            output.push_str("-------------------\n\n");
            output.push_str(&self.format_routing_section(&self.global_routing_map));
        }

        // Wildcards analysis
        output.push_str("WILDCARD ANALYSIS:\n");
        output.push_str("------------------\n");
        output.push_str(&self.analyze_wildcards());

        output.push_str("\n==================================================\n");
        output.push_str("              END OF ROUTING MAP DUMP            \n");
        output.push_str("==================================================\n");

        output
    }

    /// Format a routing map section
    fn format_routing_section(&self, map: &HashMap<String, Vec<PolicyUnit>>) -> String {
        let mut output = String::new();

        // Sort keys for consistent output
        let mut keys: Vec<_> = map.keys().cloned().collect();
        keys.sort();

        for key in keys {
            let policies = &map[&key];

            // Format the route key with visual distinction
            let route_type = if key.contains(':') {
                if key.ends_with(":*") {
                    "[WILDCARD]"
                } else {
                    "[SPECIFIC]"
                }
            } else {
                "[EVENT-ONLY]"
            };

            output.push_str(&format!("Route: {key} {route_type}\n"));
            output.push_str(&format!("  Policies ({}):\n", policies.len()));

            for (i, policy) in policies.iter().enumerate() {
                output.push_str(&format!("    {}. {}\n", i + 1, policy.package_name));
                output.push_str(&format!("       File: {}\n", policy.path.display()));

                if !policy.routing.required_events.is_empty() {
                    output.push_str(&format!(
                        "       Events: {}\n",
                        policy.routing.required_events.join(", ")
                    ));
                }

                if !policy.routing.required_tools.is_empty() {
                    output.push_str(&format!(
                        "       Tools: {}\n",
                        policy.routing.required_tools.join(", ")
                    ));
                }

                if !policy.routing.required_signals.is_empty() {
                    output.push_str(&format!(
                        "       Signals: {}\n",
                        policy.routing.required_signals.join(", ")
                    ));
                }
            }
            output.push('\n');
        }

        output
    }

    /// Analyze wildcard routing patterns
    fn analyze_wildcards(&self) -> String {
        let mut output = String::new();

        let wildcards: Vec<_> = self
            .routing_map
            .keys()
            .filter(|k| k.ends_with(":*") || !k.contains(':'))
            .cloned()
            .collect();

        if wildcards.is_empty() {
            output.push_str("  No wildcard routes found\n");
        } else {
            output.push_str(&format!("  Found {} wildcard routes:\n", wildcards.len()));
            for wc in wildcards {
                let count = self.routing_map[&wc].len();
                output.push_str(&format!("    {wc} -> {count} policies\n"));
            }
        }

        output
    }
}

mod graph;
