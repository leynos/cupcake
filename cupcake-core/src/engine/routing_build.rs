//! Routing map construction and policy parsing for the engine.

use super::*;

impl Engine {
    /// Build routing map for global policies
    pub(super) fn build_global_routing_map(&mut self) {
        Self::build_routing_map_generic(
            &self.global_policies,
            &mut self.global_routing_map,
            "global",
        );
    }

    /// Parse a single policy file to extract selector and metadata
    pub(super) async fn parse_policy(&self, path: &Path) -> Result<PolicyUnit> {
        let content = tokio::fs::read_to_string(path)
            .await
            .context("Failed to read policy file")?;

        // Extract package name
        let package_name =
            metadata::extract_package_name(&content).context("Failed to extract package name")?;

        // Parse OPA metadata
        let policy_metadata =
            metadata::parse_metadata(&content).context("Failed to parse OPA metadata")?;

        // Extract routing directive - system policies don't need routing
        let routing = if let Some(ref meta) = policy_metadata {
            if let Some(ref routing_directive) = meta.custom.routing {
                // Validate the routing directive
                metadata::validate_routing_directive(routing_directive, &package_name)
                    .with_context(|| {
                        format!("Invalid routing directive in policy {package_name}")
                    })?;
                routing_directive.clone()
            } else if package_name.starts_with("cupcake.system")
                || package_name.starts_with("cupcake.global.system")
            {
                // System packages don't need routing - they're entrypoints or helpers
                // This covers cupcake.system, cupcake.system.commands, cupcake.global.system, etc.
                debug!(
                    "System package {} has no routing directive (this is expected)",
                    package_name
                );
                RoutingDirective::default()
            } else {
                warn!(
                    "Policy {} has no routing directive in metadata - will not be routed",
                    package_name
                );
                return Err(anyhow::anyhow!("Policy missing routing directive"));
            }
        } else {
            // System packages are allowed to have no metadata
            if package_name.starts_with("cupcake.system")
                || package_name.starts_with("cupcake.global.system")
            {
                debug!(
                    "System package {} has no metadata block (this is allowed)",
                    package_name
                );
                RoutingDirective::default()
            } else {
                warn!(
                    "Policy {} has no metadata block - will not be routed",
                    package_name
                );
                return Err(anyhow::anyhow!("Policy missing metadata"));
            }
        };

        Ok(PolicyUnit {
            path: path.to_path_buf(),
            package_name,
            routing,
            metadata: policy_metadata,
        })
    }

    /// Build the routing map from parsed policies
    pub(super) fn build_routing_map(&mut self) {
        Self::build_routing_map_generic(&self.policies, &mut self.routing_map, "project");
    }

    /// Generic method to build routing maps for both global and project policies
    pub(super) fn build_routing_map_generic(
        policies: &[PolicyUnit],
        routing_map: &mut HashMap<String, Vec<PolicyUnit>>,
        map_type: &str,
    ) {
        routing_map.clear();

        for policy in policies {
            // Create routing keys for this policy from metadata
            let routing_keys = routing::create_routing_key_from_metadata(&policy.routing);

            // Add policy to the routing map for each key
            for key in routing_keys {
                routing_map
                    .entry(key.clone())
                    .or_default()
                    .push(policy.clone());
                debug!(
                    "Added {} policy {} to routing key: {}",
                    map_type, policy.package_name, key
                );
            }
        }

        // Handle wildcard routes - also add them to specific tool lookups
        // This allows "PreToolUse:Bash" to find both specific and wildcard policies
        let wildcard_keys: Vec<String> = routing_map
            .keys()
            .filter(|k| k.ends_with(":*"))
            .cloned()
            .collect();

        for wildcard_key in wildcard_keys {
            if let Some(wildcard_policies) = routing_map.get(&wildcard_key).cloned() {
                let event_prefix = wildcard_key.strip_suffix(":*").unwrap();

                // Find all specific tool keys for this event
                let specific_keys: Vec<String> = routing_map
                    .keys()
                    .filter(|k| k.starts_with(&format!("{event_prefix}:")) && !k.ends_with(":*"))
                    .cloned()
                    .collect();

                // Add wildcard policies to each specific tool key
                for specific_key in specific_keys {
                    routing_map
                        .entry(specific_key)
                        .or_default()
                        .extend(wildcard_policies.clone());
                }
            }
        }

        // Handle event-only policies for tool events (PreToolUse, PostToolUse)
        // These events ALWAYS have tools, so event-only policies are effectively wildcards
        const TOOL_EVENTS: &[&str] = &["PreToolUse", "PostToolUse"];

        for tool_event in TOOL_EVENTS {
            if let Some(event_only_policies) = routing_map.get(*tool_event).cloned() {
                // Find all specific tool keys for this event
                let specific_keys: Vec<String> = routing_map
                    .keys()
                    .filter(|k| k.starts_with(&format!("{tool_event}:")) && !k.ends_with(":*"))
                    .cloned()
                    .collect();

                // Add event-only policies to each specific tool key (they act as wildcards)
                for specific_key in specific_keys {
                    routing_map
                        .entry(specific_key.clone())
                        .and_modify(|policies| {
                            // Add event-only policies if not already present
                            for event_policy in &event_only_policies {
                                if !policies
                                    .iter()
                                    .any(|p| p.package_name == event_policy.package_name)
                                {
                                    policies.push(event_policy.clone());
                                    debug!(
                                        "Added {} wildcard policy {} to specific key: {}",
                                        map_type, event_policy.package_name, specific_key
                                    );
                                }
                            }
                        });
                }
            }
        }

        // Log the routing map for verification
        for (key, policies) in routing_map {
            debug!(
                "Route '{}' -> {} policies: [{}]",
                key,
                policies.len(),
                policies
                    .iter()
                    .map(|p| p.package_name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
}
