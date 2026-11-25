//! The `eval` command: read a hook event from stdin, evaluate it against the
//! policies and print the harness-specific response.

use anyhow::{Context, Result};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info};

use cupcake_core::{engine, harness, telemetry::TelemetryContext};

/// Evaluate the hook event on stdin and print the harness response.
pub(crate) async fn eval_command(
    policy_dir: PathBuf,
    strict: bool,
    debug_files_enabled: bool,
    debug_dir: Option<PathBuf>,
    engine_config: engine::EngineConfig,
) -> Result<()> {
    // Get the harness type from engine_config for later use
    let harness_type = engine_config.harness;

    // Read hook event from stdin and parse JSON once (reused for path resolution and evaluation)
    let (stdin_buffer, mut hook_event_json) = read_hook_event()?;

    // TELEMETRY: Capture raw event BEFORE any preprocessing mutations
    let raw_event_for_telemetry = hook_event_json.clone();

    info!("Processing harness: {:?}", harness_type);
    debug!("Parsing hook event from stdin");

    // Policy directory is resolved relative to the process cwd
    // All harnesses (Claude Code, Cursor, etc.) spawn hooks with cwd set to the project root
    let resolved_policy_dir = policy_dir;

    debug!(
        "Initializing Cupcake engine with policies from: {:?}",
        resolved_policy_dir
    );

    // Initialize the engine with configuration - MUST succeed or we exit
    let engine = new_engine_or_exit(&resolved_policy_dir, engine_config).await;

    // Apply input preprocessing to normalize adversarial patterns
    // This protects all policies (user and builtin) from spacing bypasses
    let preprocess_start = std::time::Instant::now();
    let preprocess_config = cupcake_core::preprocessing::PreprocessConfig::default();
    let preprocess_result = cupcake_core::preprocessing::preprocess_input(
        &mut hook_event_json,
        &preprocess_config,
        harness_type,
    );
    let preprocess_duration_us = preprocess_start.elapsed().as_micros() as u64;
    debug!(
        "Input preprocessing completed in {}μs",
        preprocess_duration_us
    );

    // Add hookEventName field for the engine if not present (for routing compatibility)
    // The engine routing needs this field
    add_hook_event_name(&mut hook_event_json);

    // Get telemetry config from the engine
    let telemetry_config = engine.telemetry_config().cloned();
    let telemetry_enabled = telemetry_config
        .as_ref()
        .map(|t| t.enabled)
        .unwrap_or(false);

    // Create TelemetryContext if enabled via CLI flag OR telemetry config
    // This captures raw event (before preprocessing) and will track enrichment + evaluation
    let mut telemetry_ctx = if debug_files_enabled || telemetry_enabled {
        Some(build_telemetry_context(
            raw_event_for_telemetry,
            harness_type,
            TelemetrySettings {
                debug_files_enabled,
                debug_dir,
                telemetry_config,
            },
            EnrichmentRecord {
                event: hook_event_json.clone(),
                applied_operations: preprocess_result.applied_operations.clone(),
                duration_us: preprocess_duration_us,
            },
        ))
    } else {
        None
    };

    // Evaluate policies for this hook event
    let decision = match engine
        .evaluate(&hook_event_json, telemetry_ctx.as_mut())
        .await
    {
        Ok(d) => d,
        Err(e) => {
            error!("Policy evaluation failed: {:#}", e);

            // Capture the error in telemetry
            if let Some(ref mut ctx) = telemetry_ctx {
                ctx.add_error(format!("Policy evaluation failed: {e:#}"));
                // Note: We exit early here without calling finalize().
                // Drop impl will write telemetry as a fallback safety net.
            }

            // On error, return a safe "allow" with no modifications
            // This ensures we don't break the agent on engine failures
            println!("{{}}");
            if strict {
                std::process::exit(1);
            }
            return Ok(());
        }
    };

    // Format response based on harness type from engine config
    //
    // NOTE: We re-parse stdin_buffer into typed event structs here. This is intentional:
    // - Earlier we parsed to serde_json::Value for generic engine evaluation
    // - Here we need strongly-typed event structs for type-safe response formatting
    // - The format_response() methods require specific event types (ClaudeCodeEvent, CursorEvent, etc.)
    // - Attempting to convert Value -> typed struct would require the same deserialization work
    let response = format_harness_response(harness_type, &stdin_buffer, &decision)?;

    // Finalize telemetry with response - this is the primary write mechanism.
    // (Drop impl provides fallback if finalize() isn't called, e.g., on early error exit)
    if let Some(ref mut ctx) = telemetry_ctx {
        if let Err(e) = ctx.finalize(Some(response.clone())) {
            debug!("Failed to finalize telemetry: {}", e);
        }
    }

    // Output the response to stdout as JSON string
    println!("{}", serde_json::to_string(&response)?);

    // In strict mode, exit non-zero on blocking decisions
    let stops_agent = decision.is_halt() || decision.is_blocking();
    if strict && stops_agent {
        std::process::exit(1);
    }

    Ok(())
}

/// Read the hook event from stdin, returning the raw text and its parsed JSON.
fn read_hook_event() -> Result<(String, serde_json::Value)> {
    let mut stdin_buffer = String::new();
    io::stdin()
        .read_to_string(&mut stdin_buffer)
        .context("Failed to read hook event from stdin")?;

    let hook_event_json: serde_json::Value =
        serde_json::from_str(&stdin_buffer).context("Failed to parse hook event JSON")?;
    Ok((stdin_buffer, hook_event_json))
}

/// Build the engine, or print set-up guidance and exit with status 1 on failure.
async fn new_engine_or_exit(
    policy_dir: &Path,
    engine_config: engine::EngineConfig,
) -> engine::Engine {
    match engine::Engine::new_with_config(policy_dir, engine_config).await {
        Ok(e) => {
            debug!("Engine initialized successfully");
            e
        }
        Err(e) => {
            error!("Fatal: Cupcake engine failed to initialize: {:#}", e);
            eprintln!("\nError: Could not start the Cupcake engine.");
            eprintln!(
                "Please ensure the OPA CLI is installed and accessible in your system's PATH."
            );
            eprintln!("You can download it from: https://www.openpolicyagent.org/docs/latest/#running-opa");
            std::process::exit(1);
        }
    }
}

/// Mirror `hook_event_name` into `hookEventName` for engine routing when only one is absent.
fn add_hook_event_name(hook_event_json: &mut serde_json::Value) {
    let Some(obj) = hook_event_json.as_object_mut() else {
        return;
    };
    if obj.contains_key("hookEventName") || obj.contains_key("hook_event_name") {
        return;
    }
    // Extract event name from the JSON
    if let Some(event_name) = obj.get("hook_event_name").and_then(|v| v.as_str()) {
        obj.insert(
            "hookEventName".to_string(),
            serde_json::Value::String(event_name.to_string()),
        );
    }
}

/// Where a telemetry context writes its output.
struct TelemetrySettings {
    debug_files_enabled: bool,
    debug_dir: Option<PathBuf>,
    telemetry_config: Option<engine::TelemetryConfig>,
}

/// The result of input preprocessing, as recorded in telemetry.
struct EnrichmentRecord {
    event: serde_json::Value,
    applied_operations: Vec<String>,
    duration_us: u64,
}

/// Create a telemetry context holding the raw event and the preprocessing record.
fn build_telemetry_context(
    raw_event: serde_json::Value,
    harness_type: cupcake_core::harness::types::HarnessType,
    settings: TelemetrySettings,
    enrichment: EnrichmentRecord,
) -> TelemetryContext {
    let trace_id = cupcake_core::engine::trace::generate_trace_id();
    let mut ctx = TelemetryContext::new(raw_event, harness_type, trace_id);

    // Configure output destinations
    ctx.configure(
        settings.debug_files_enabled,
        settings.debug_dir,
        settings.telemetry_config,
    );

    // Record enrichment (preprocessing results) - use actual operations applied
    ctx.record_enrichment(
        enrichment.event,
        enrichment.applied_operations,
        enrichment.duration_us,
    );
    ctx
}

/// Re-parse the raw event into the harness's typed event and format the decision for it.
///
/// The typed parse is intentional: `format_response` needs the concrete event type, while
/// the engine evaluates the generic JSON value.
fn format_harness_response(
    harness_type: cupcake_core::harness::types::HarnessType,
    stdin_buffer: &str,
    decision: &engine::decision::FinalDecision,
) -> Result<serde_json::Value> {
    use cupcake_core::harness::types::HarnessType;
    match harness_type {
        HarnessType::ClaudeCode => {
            parse_and_format::<harness::events::claude_code::ClaudeCodeEvent>(
                stdin_buffer,
                decision,
                harness::ClaudeHarness::format_response,
            )
        }
        HarnessType::Cursor => parse_and_format::<harness::events::cursor::CursorEvent>(
            stdin_buffer,
            decision,
            harness::CursorHarness::format_response,
        ),
        HarnessType::Factory => parse_and_format::<harness::events::factory::FactoryEvent>(
            stdin_buffer,
            decision,
            harness::FactoryHarness::format_response,
        ),
        HarnessType::OpenCode => parse_and_format::<harness::events::opencode::OpenCodeEvent>(
            stdin_buffer,
            decision,
            harness::OpenCodeHarness::format_response,
        ),
    }
}

/// Parse `stdin_buffer` as the harness event type `E` and format the decision with `format`.
fn parse_and_format<E: serde::de::DeserializeOwned>(
    stdin_buffer: &str,
    decision: &engine::decision::FinalDecision,
    format: fn(&E, &engine::decision::FinalDecision) -> Result<serde_json::Value>,
) -> Result<serde_json::Value> {
    let event = serde_json::from_str::<E>(stdin_buffer)?;
    format(&event, decision)
}

#[cfg(test)]
mod tests {
    //! Tests for the `eval` command's telemetry and response-formatting helpers.

    use super::*;
    use cupcake_core::harness::types::HarnessType;
    use serde_json::json;

    const ALL_HARNESSES: [HarnessType; 4] = [
        HarnessType::ClaudeCode,
        HarnessType::Cursor,
        HarnessType::Factory,
        HarnessType::OpenCode,
    ];

    fn allow() -> engine::decision::FinalDecision {
        engine::decision::FinalDecision::Allow {
            context: Vec::new(),
        }
    }

    #[test]
    fn format_harness_response_rejects_unparseable_events_for_every_harness() {
        for harness_type in ALL_HARNESSES {
            let result = format_harness_response(harness_type, "not json", &allow());
            assert!(
                result.is_err(),
                "{harness_type:?} accepted an invalid event"
            );
        }
    }

    #[test]
    fn format_harness_response_rejects_events_of_the_wrong_shape_for_every_harness() {
        for harness_type in ALL_HARNESSES {
            let result = format_harness_response(harness_type, "{}", &allow());
            assert!(
                result.is_err(),
                "{harness_type:?} accepted an event with no event name"
            );
        }
    }

    #[test]
    fn format_harness_response_formats_a_claude_code_event() {
        let event = json!({
            "hook_event_name": "Stop",
            "session_id": "session",
            "transcript_path": "/tmp/transcript",
            "cwd": "/tmp",
            "stop_hook_active": false
        })
        .to_string();

        let result = format_harness_response(HarnessType::ClaudeCode, &event, &allow());

        assert_eq!(result.expect("allow formats"), json!({}));

        let blocked = format_harness_response(
            HarnessType::ClaudeCode,
            &event,
            &engine::decision::FinalDecision::Block {
                reason: "no".to_string(),
                agent_messages: Vec::new(),
            },
        );
        assert_eq!(
            blocked.expect("block formats"),
            json!({"decision": "block", "reason": "no"})
        );
    }

    #[test]
    fn build_telemetry_context_keeps_the_event_harness_and_enrichment() {
        let raw = json!({"tool_name": "Bash"});
        let enriched = json!({"tool_name": "Bash", "normalized": true});
        // Created before `ctx` so it outlives the debug file written when `ctx` drops.
        let debug_dir = tempfile::tempdir().expect("temporary debug directory");

        let ctx = build_telemetry_context(
            raw.clone(),
            HarnessType::Cursor,
            TelemetrySettings {
                debug_files_enabled: true,
                debug_dir: Some(debug_dir.path().to_path_buf()),
                telemetry_config: None,
            },
            EnrichmentRecord {
                event: enriched.clone(),
                applied_operations: vec!["whitespace".to_string()],
                duration_us: 42,
            },
        );

        assert_eq!(ctx.span.raw_event, raw);
        assert!(matches!(ctx.span.harness, HarnessType::Cursor));
        assert!(ctx.has_output_configured());
        let enrich = ctx.span.enrich.as_ref().expect("enrichment recorded");
        assert_eq!(enrich.enriched_event, enriched);
        assert_eq!(enrich.operations, ["whitespace"]);
        assert_eq!(enrich.duration_us, 42);
    }

    #[test]
    fn add_hook_event_name_leaves_events_that_already_carry_a_name_unchanged() {
        let mut event = json!({"hook_event_name": "PreToolUse"});
        add_hook_event_name(&mut event);
        assert_eq!(event, json!({"hook_event_name": "PreToolUse"}));

        let mut scalar = json!("text");
        add_hook_event_name(&mut scalar);
        assert_eq!(scalar, json!("text"));
    }
}
