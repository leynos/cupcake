//! Tests for telemetry spans and their phases.

use super::*;
use serde_json::json;

#[test]
fn test_cupcake_span_creation() {
    let raw = json!({"hook_event_name": "PreToolUse", "tool_name": "Bash"});
    let span = CupcakeSpan::new(raw.clone(), HarnessType::ClaudeCode, "trace-123".into());

    assert_eq!(span.trace_id, "trace-123");
    assert_eq!(span.raw_event, raw);
    assert_eq!(span.harness, HarnessType::ClaudeCode);
    assert_eq!(span.span_id.len(), 16);
    assert!(span.start_time_unix_nano > 0);
    assert_eq!(span.end_time_unix_nano, 0); // Not finalized yet
}

#[test]
fn test_cupcake_span_finalize() {
    let raw = json!({"test": true});
    let mut span = CupcakeSpan::new(raw, HarnessType::ClaudeCode, "trace-456".into());

    assert_eq!(span.end_time_unix_nano, 0);
    span.finalize(Some(json!({"decision": "allow"})));

    assert!(span.end_time_unix_nano > span.start_time_unix_nano);
    assert!(span.response.is_some());
}

#[test]
fn test_enrich_phase() {
    let enriched = json!({"resolved_file_path": "/canonical/path"});
    let phase = EnrichPhase::new(
        enriched.clone(),
        vec!["symlink_resolution".into()],
        150,
        "parent123".into(),
        1_000_000_000_000,
    );

    assert_eq!(phase.enriched_event, enriched);
    assert_eq!(phase.operations, vec!["symlink_resolution"]);
    assert_eq!(phase.duration_us, 150);
    assert_eq!(phase.span_id.len(), 16);
}

#[test]
fn test_policy_phase() {
    let mut phase = PolicyPhase::new("project", "parent456".into());

    assert_eq!(phase.name, "project");
    assert_eq!(phase.span_id.len(), 16);

    phase
        .evaluation_mut()
        .record_routing(true, &["policy.a".into()]);
    assert!(phase.evaluation.routed);

    std::thread::sleep(std::time::Duration::from_millis(1));
    phase.finalize();
    assert!(phase.duration_ms >= 1);
}

#[test]
fn test_signals_phase() {
    let mut signals = SignalsPhase::new("parent789".into(), 1_000_000_000_000);

    signals.add_signal(SignalExecution {
        name: "git_status".into(),
        command: "git status --porcelain".into(),
        result: json!(["M src/main.rs"]),
        duration_ms: Some(45),
        exit_code: Some(0),
    });

    assert_eq!(signals.signals.len(), 1);
    assert_eq!(signals.signals[0].name, "git_status");
}

#[test]
fn test_full_trace_structure() {
    // Build a complete trace
    let raw = json!({"hook_event_name": "PreToolUse", "tool_name": "Bash"});
    let mut span = CupcakeSpan::new(raw, HarnessType::ClaudeCode, "trace-full".into());

    // Add enrich phase
    span.set_enrich(EnrichPhase::new(
        json!({"enriched": true}),
        vec!["whitespace_normalization".into()],
        100,
        span.span_id().to_string(),
        span.start_time_unix_nano,
    ));

    // Add policy phase
    let mut phase = PolicyPhase::new("project", span.span_id().to_string());

    // Add signals to phase
    let mut signals = SignalsPhase::new(phase.span_id().to_string(), phase.start_time_unix_nano);
    signals.add_signal(SignalExecution {
        name: "git_status".into(),
        command: "git status".into(),
        result: json!([]),
        duration_ms: Some(10),
        exit_code: Some(0),
    });
    signals.finalize(10);
    phase.set_signals(signals);

    // Add evaluation result
    phase
        .evaluation_mut()
        .record_routing(true, &["policy.test".into()]);
    phase
        .evaluation_mut()
        .record_exit("No policies matched - implicit allow");
    phase.finalize();

    span.add_phase(phase);
    span.finalize(Some(json!({"decision": "allow"})));

    // Verify structure
    assert!(span.enrich.is_some());
    assert_eq!(span.phases.len(), 1);
    assert!(span.phases[0].signals.is_some());
    assert_eq!(span.phases[0].signals.as_ref().unwrap().signals.len(), 1);
    assert!(span.response.is_some());

    // Verify serialization works
    let json = serde_json::to_string_pretty(&span).expect("serialize");
    assert!(json.contains("\"trace_id\": \"trace-full\""));
    assert!(json.contains("\"name\": \"project\""));
    assert!(json.contains("\"git_status\""));
}
