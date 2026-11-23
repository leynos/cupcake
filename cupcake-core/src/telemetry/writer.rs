//! Telemetry output writers.
//!
//! Handles writing CupcakeSpan to various destinations:
//! - Debug files (.cupcake/debug/)
//! - Telemetry files (configurable destination)

use anyhow::Result;
use chrono::{DateTime, Local};
use std::path::Path;

use crate::engine::rulebook::TelemetryFormat;

use super::span::{CupcakeSpan, PolicyPhase, SignalsPhase};
use crate::engine::decision::{DecisionObject, DecisionSet};

/// Telemetry output writer.
pub struct TelemetryWriter;

impl TelemetryWriter {
    /// Write telemetry to debug file in .cupcake/debug/.
    pub fn write_debug_file(span: &CupcakeSpan, debug_dir: &Path) -> Result<()> {
        if !debug_dir.exists() {
            crate::fs_access::create_dir_all(debug_dir)?;
        }

        let datetime: DateTime<Local> = span.timestamp.into();
        let filename = format!(
            "{}_{}.txt",
            datetime.format("%Y-%m-%d_%H-%M-%S"),
            span.trace_id
        );

        let file_path = debug_dir.join(filename);
        let content = Self::format_human_readable(span)?;

        crate::fs_access::write(file_path, content)?;
        Ok(())
    }

    /// Write telemetry to configured destination.
    pub fn write_telemetry(
        span: &CupcakeSpan,
        format: &TelemetryFormat,
        destination: &Path,
    ) -> Result<()> {
        if !destination.exists() {
            crate::fs_access::create_dir_all(destination)?;
        }

        let datetime: DateTime<Local> = span.timestamp.into();
        let extension = match format {
            TelemetryFormat::Json => "json",
            TelemetryFormat::Text => "txt",
        };
        let filename = format!(
            "{}_{}.{}",
            datetime.format("%Y-%m-%d_%H-%M-%S"),
            span.trace_id,
            extension
        );

        let file_path = destination.join(filename);

        let content = match format {
            TelemetryFormat::Json => format!("{}\n", serde_json::to_string(span)?),
            TelemetryFormat::Text => Self::format_human_readable(span)?,
        };

        crate::fs_access::write(file_path, content)?;
        Ok(())
    }

    /// Format telemetry as human-readable text.
    fn format_human_readable(span: &CupcakeSpan) -> Result<String> {
        let datetime: DateTime<Local> = span.timestamp.into();
        let start_time = datetime.format("%Y-%m-%d %H:%M:%S").to_string();

        let mut out = String::new();

        // Header
        out.push_str(&format!(
            "===== Cupcake Trace [{}] [{}] =====\n",
            start_time, span.trace_id
        ));
        out.push_str(&format!("Harness: {:?}\n", span.harness));
        out.push_str(&format!("Duration: {}ms\n\n", span.total_duration_ms));

        // Raw Event
        out.push_str("----- RAW EVENT -----\n");
        out.push_str(&serde_json::to_string_pretty(&span.raw_event)?);
        out.push_str("\n\n");

        Self::push_enrich_section(&mut out, span)?;
        Self::push_policy_phases(&mut out, span)?;
        Self::push_response_section(&mut out, span)?;
        Self::push_errors_section(&mut out, span);

        out.push_str(&format!("===== End [{}ms] =====\n", span.total_duration_ms));

        Ok(out)
    }

    /// Append the enrich phase, or a note that it was not recorded.
    fn push_enrich_section(out: &mut String, span: &CupcakeSpan) -> Result<()> {
        out.push_str("----- ENRICH -----\n");
        let Some(ref enrich) = span.enrich else {
            out.push_str("(not recorded)\n\n");
            return Ok(());
        };
        out.push_str(&format!("Operations: {}\n", enrich.operations.join(", ")));
        out.push_str(&format!("Duration: {}μs\n", enrich.duration_us));
        out.push_str("Enriched:\n");
        out.push_str(&serde_json::to_string_pretty(&enrich.enriched_event)?);
        out.push_str("\n\n");
        Ok(())
    }

    /// Append every policy phase, or a note that evaluation exited early.
    fn push_policy_phases(out: &mut String, span: &CupcakeSpan) -> Result<()> {
        if span.phases.is_empty() {
            out.push_str("----- POLICY PHASES -----\n");
            out.push_str("(no phases - early exit before evaluation)\n\n");
            return Ok(());
        }
        for (i, phase) in span.phases.iter().enumerate() {
            Self::push_phase(out, i + 1, phase)?;
        }
        Ok(())
    }

    /// Append one numbered policy phase: signals, evaluation and duration.
    fn push_phase(out: &mut String, number: usize, phase: &PolicyPhase) -> Result<()> {
        out.push_str(&format!(
            "----- PHASE {}: {} ({}) -----\n",
            number,
            phase.name.to_uppercase(),
            phase.span_id
        ));

        Self::push_signals(out, phase.signals.as_ref())?;
        Self::push_evaluation(out, phase);

        out.push_str(&format!("Phase Duration: {}ms\n\n", phase.duration_ms));
        Ok(())
    }

    /// Append the signals collected in a phase, or `Signals: none`.
    fn push_signals(out: &mut String, signals: Option<&SignalsPhase>) -> Result<()> {
        let Some(signals) = signals else {
            out.push_str("Signals: none\n");
            return Ok(());
        };
        out.push_str(&format!(
            "Signals: {} collected in {}ms\n",
            signals.signals.len(),
            signals.duration_ms
        ));
        for sig in &signals.signals {
            out.push_str(&format!("  - {}: {}", sig.name, sig.command));
            if let Some(ms) = sig.duration_ms {
                out.push_str(&format!(" ({ms}ms)"));
            }
            if let Some(code) = sig.exit_code {
                out.push_str(&format!(" [exit {code}]"));
            }
            out.push('\n');
            // Show result if small
            let result_str = serde_json::to_string(&sig.result)?;
            if result_str.len() < 200 {
                out.push_str(&format!("    → {result_str}\n"));
            }
        }
        Ok(())
    }

    /// Append a phase's evaluation: routing, matches, exit reason, WASM results and decision.
    fn push_evaluation(out: &mut String, phase: &PolicyPhase) {
        let eval = &phase.evaluation;
        out.push_str(&format!("Routed: {}\n", eval.routed));

        if !eval.matched_policies.is_empty() {
            out.push_str(&format!("Matched: {}\n", eval.matched_policies.join(", ")));
        }

        if let Some(ref exit) = eval.exit_reason {
            out.push_str(&format!("Exit: {exit}\n"));
        }

        if let Some(ref ds) = eval.wasm_decision_set {
            Self::push_wasm_results(out, ds);
        }

        if let Some(ref decision) = eval.final_decision {
            out.push_str(&format!("Decision: {decision:?}\n"));
        }
    }

    /// Append the WASM decision set, skipping each empty category.
    fn push_wasm_results(out: &mut String, ds: &DecisionSet) {
        out.push_str("WASM Results:\n");
        Self::push_decision_group(out, "Halts", &ds.halts);
        Self::push_decision_group(out, "Denials", &ds.denials);
        Self::push_decision_group(out, "Blocks", &ds.blocks);
        Self::push_decision_group(out, "Asks", &ds.asks);
        Self::push_count_line(out, "Modifications", ds.modifications.len());
        Self::push_count_line(out, "Context", ds.add_context.len());
    }

    /// Append a labelled count and each decision, or nothing when there are none.
    fn push_decision_group(out: &mut String, label: &str, decisions: &[DecisionObject]) {
        if decisions.is_empty() {
            return;
        }
        out.push_str(&format!("  {label}: {}\n", decisions.len()));
        for d in decisions {
            out.push_str(&format!(
                "    [{}] {} ({})\n",
                d.rule_id, d.reason, d.severity
            ));
        }
    }

    /// Append a labelled count, or nothing when it is zero.
    fn push_count_line(out: &mut String, label: &str, count: usize) {
        if count > 0 {
            out.push_str(&format!("  {label}: {count}\n"));
        }
    }

    /// Append the final response, or `(none)`.
    fn push_response_section(out: &mut String, span: &CupcakeSpan) -> Result<()> {
        out.push_str("----- RESPONSE -----\n");
        match span.response {
            Some(ref response) => {
                out.push_str(&serde_json::to_string_pretty(response)?);
                out.push('\n');
            }
            None => out.push_str("(none)\n"),
        }
        out.push('\n');
        Ok(())
    }

    /// Append the numbered errors, if any.
    fn push_errors_section(out: &mut String, span: &CupcakeSpan) {
        if span.errors.is_empty() {
            return;
        }
        out.push_str("----- ERRORS -----\n");
        for (i, err) in span.errors.iter().enumerate() {
            out.push_str(&format!("{}. {}\n", i + 1, err));
        }
        out.push('\n');
    }
}

#[cfg(test)]
mod tests {
    //! Tests for the `writer` module.

    use super::*;
    use crate::fs_access;
    use crate::harness::types::HarnessType;
    use crate::telemetry::{EnrichPhase, PolicyPhase, SignalExecution, SignalsPhase};
    use serde_json::json;
    use tempfile::tempdir;

    fn make_test_span() -> CupcakeSpan {
        let mut span = CupcakeSpan::new(
            json!({"hook_event_name": "PreToolUse", "tool_name": "Bash"}),
            HarnessType::ClaudeCode,
            "test-trace".into(),
        );

        span.set_enrich(EnrichPhase::new(
            json!({"enriched": true}),
            vec!["whitespace_normalization".into()],
            100,
            span.span_id().to_string(),
            span.start_time_unix_nano,
        ));

        let mut phase = PolicyPhase::new("project", span.span_id().to_string());

        let mut signals =
            SignalsPhase::new(phase.span_id().to_string(), phase.start_time_unix_nano);
        signals.add_signal(SignalExecution {
            name: "git_status".into(),
            command: "git status --porcelain".into(),
            result: json!(["M src/main.rs"]),
            duration_ms: Some(25),
            exit_code: Some(0),
        });
        signals.finalize(25);
        phase.set_signals(signals);

        phase.evaluation_mut().record_routing(false, &[]);
        phase.evaluation_mut().record_exit("No policies matched");
        phase.finalize();

        span.add_phase(phase);
        span.finalize(Some(json!({"decision": "allow"})));

        span
    }

    #[test]
    fn test_write_debug_file() {
        let span = make_test_span();
        let dir = tempdir().unwrap();

        TelemetryWriter::write_debug_file(&span, dir.path()).unwrap();

        let files = fs_access::read_dir(dir.path()).unwrap();
        assert_eq!(files.len(), 1);

        let content = fs_access::read_to_string(&files[0]).unwrap();
        assert!(content.contains("Cupcake Trace"));
        assert!(content.contains("test-trace"));
        assert!(content.contains("git_status"));
    }

    #[test]
    fn test_write_telemetry_json() {
        let span = make_test_span();
        let dir = tempdir().unwrap();

        TelemetryWriter::write_telemetry(&span, &TelemetryFormat::Json, dir.path()).unwrap();

        let files: Vec<_> = fs_access::read_dir(dir.path())
            .unwrap()
            .into_iter()
            .filter(|path| path.extension().map(|s| s == "json").unwrap_or(false))
            .collect();
        assert_eq!(files.len(), 1);

        let content = fs_access::read_to_string(&files[0]).unwrap();
        let _: serde_json::Value = serde_json::from_str(&content).unwrap();
    }

    #[test]
    fn test_human_readable_format() {
        let span = make_test_span();
        let output = TelemetryWriter::format_human_readable(&span).unwrap();

        assert!(output.contains("RAW EVENT"));
        assert!(output.contains("ENRICH"));
        assert!(output.contains("PHASE 1: PROJECT"));
        assert!(output.contains("Signals: 1 collected"));
        assert!(output.contains("git_status"));
        assert!(output.contains("RESPONSE"));
    }
}
