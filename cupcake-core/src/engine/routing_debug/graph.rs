//! Graphviz output and routing statistics for the routing diagnostics.

use super::*;

impl Engine {
    /// Write Graphviz DOT file for visualization
    pub(super) fn write_dot_graph(
        &self,
        debug_dir: &Path,
        timestamp: &impl std::fmt::Display,
    ) -> Result<()> {
        let dot_file = debug_dir.join(format!("routing_map_{timestamp}.dot"));
        let dot_graph = self.generate_routing_graph_dot();
        crate::fs_access::write(dot_file, dot_graph)?;
        Ok(())
    }

    /// Generate Graphviz DOT format for routing visualization
    fn generate_routing_graph_dot(&self) -> String {
        let mut dot = String::new();

        dot.push_str("digraph RoutingMap {\n");
        dot.push_str("  rankdir=LR;\n");
        dot.push_str("  node [shape=box, style=rounded];\n");
        dot.push_str("  edge [fontsize=10];\n\n");

        // Title
        dot.push_str(&format!(
            "  label=\"Cupcake Routing Map - {}\";\n",
            Local::now().format("%Y-%m-%d %H:%M:%S")
        ));
        dot.push_str("  fontsize=16;\n\n");

        // Collect unique events and tools
        let mut events = HashSet::new();
        let mut tools = HashSet::new();

        for key in self.routing_map.keys() {
            if let Some(colon_pos) = key.find(':') {
                events.insert(&key[..colon_pos]);
                let tool_part = &key[colon_pos + 1..];
                if tool_part != "*" {
                    tools.insert(tool_part);
                }
            } else {
                events.insert(key.as_str());
            }
        }

        // Events cluster
        dot.push_str("  subgraph cluster_events {\n");
        dot.push_str("    label=\"Events\";\n");
        dot.push_str("    style=filled;\n");
        dot.push_str("    color=lightgrey;\n");
        dot.push_str("    node [shape=ellipse, style=filled, fillcolor=lightyellow];\n");

        for event in events {
            dot.push_str(&format!("    \"event_{event}\" [label=\"{event}\"];\n"));
        }
        dot.push_str("  }\n\n");

        // Tools cluster
        if !tools.is_empty() {
            dot.push_str("  subgraph cluster_tools {\n");
            dot.push_str("    label=\"Tools\";\n");
            dot.push_str("    style=filled;\n");
            dot.push_str("    color=lightgreen;\n");
            dot.push_str("    node [shape=diamond, style=filled, fillcolor=lightgreen];\n");

            for tool in tools {
                dot.push_str(&format!("    \"tool_{tool}\" [label=\"{tool}\"];\n"));
            }
            dot.push_str("  }\n\n");
        }

        // Policies cluster
        dot.push_str("  subgraph cluster_policies {\n");
        dot.push_str("    label=\"Policies\";\n");
        dot.push_str("    style=filled;\n");
        dot.push_str("    color=lightblue;\n");
        dot.push_str("    node [shape=box, style=filled, fillcolor=lightblue];\n");

        // Use simplified names for policies
        for policy in &self.policies {
            let short_name = policy
                .package_name
                .replace("cupcake.policies.", "")
                .replace("cupcake.global.policies.", "global.");
            dot.push_str(&format!(
                "    \"policy_{}\" [label=\"{}\"];\n",
                policy.package_name, short_name
            ));
        }
        dot.push_str("  }\n\n");

        // Add edges
        for (key, policies) in &self.routing_map {
            for policy in policies {
                if let Some(colon_pos) = key.find(':') {
                    let event = &key[..colon_pos];
                    let tool = &key[colon_pos + 1..];

                    if tool == "*" {
                        // Wildcard: event -> policy
                        dot.push_str(&format!(
                            "  \"event_{}\" -> \"policy_{}\" [label=\"*\", style=dashed];\n",
                            event, policy.package_name
                        ));
                    } else {
                        // Specific: event -> tool -> policy
                        dot.push_str(&format!("  \"event_{event}\" -> \"tool_{tool}\";\n"));
                        dot.push_str(&format!(
                            "  \"tool_{}\" -> \"policy_{}\";\n",
                            tool, policy.package_name
                        ));
                    }
                } else {
                    // Event only
                    dot.push_str(&format!(
                        "  \"event_{}\" -> \"policy_{}\";\n",
                        key, policy.package_name
                    ));
                }
            }
        }

        dot.push_str("}\n");
        dot
    }

    /// Compute routing statistics
    pub(super) fn compute_routing_statistics(&self) -> RoutingStatistics {
        let total_routes = self.routing_map.len() + self.global_routing_map.len();

        let wildcard_routes = self
            .routing_map
            .keys()
            .chain(self.global_routing_map.keys())
            .filter(|k| k.ends_with(":*") || !k.contains(':'))
            .count();

        let specific_routes = total_routes - wildcard_routes;

        // Collect unique events and tools
        let mut events = HashSet::new();
        let mut tools = HashSet::new();

        for key in self
            .routing_map
            .keys()
            .chain(self.global_routing_map.keys())
        {
            if let Some(colon_pos) = key.find(':') {
                events.insert(key[..colon_pos].to_string());
                let tool_part = &key[colon_pos + 1..];
                if tool_part != "*" {
                    tools.insert(tool_part.to_string());
                }
            } else {
                events.insert(key.clone());
            }
        }

        // Calculate average policies per route
        let total_policy_mappings: usize = self
            .routing_map
            .values()
            .chain(self.global_routing_map.values())
            .map(|v| v.len())
            .sum();

        let average_policies_per_route = if total_routes > 0 {
            total_policy_mappings as f64 / total_routes as f64
        } else {
            0.0
        };

        RoutingStatistics {
            total_routes,
            wildcard_routes,
            specific_routes,
            events_covered: events.into_iter().collect(),
            tools_covered: tools.into_iter().collect(),
            average_policies_per_route,
        }
    }
}
