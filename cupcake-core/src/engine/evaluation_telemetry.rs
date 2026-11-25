//! Telemetry and early-termination helpers for `Engine::evaluate`.

use super::decision::FinalDecision;
use crate::telemetry::{PolicyPhase, TelemetryContext};

/// Run `record` on the current telemetry phase, when telemetry is enabled and
/// a phase has started.
pub(super) fn with_current_phase(
    telemetry: &mut Option<&mut TelemetryContext>,
    record: impl FnOnce(&mut PolicyPhase),
) {
    if let Some(ctx) = telemetry.as_mut() {
        if let Some(phase) = ctx.current_phase_mut() {
            record(phase);
        }
    }
}

/// A global decision that ends evaluation before project policies run.
pub(super) struct GlobalStop<'a> {
    /// Upper-case verb used in the log line, such as `HALT`.
    pub(super) verb_upper: &'static str,
    /// Lower-case verb used in the telemetry exit reason, such as `halt`.
    pub(super) verb_lower: &'static str,
    /// Value recorded in the span's `final_decision` field.
    pub(super) span_label: &'static str,
    /// The decision's reason.
    pub(super) reason: &'a str,
}

/// Classify a global decision as a stop, or `None` when evaluation proceeds.
pub(super) fn global_stop(decision: &FinalDecision) -> Option<GlobalStop<'_>> {
    match decision {
        FinalDecision::Halt { reason, .. } => Some(GlobalStop {
            verb_upper: "HALT",
            verb_lower: "halt",
            span_label: "GlobalHalt",
            reason,
        }),
        FinalDecision::Deny { reason, .. } => Some(GlobalStop {
            verb_upper: "DENY",
            verb_lower: "deny",
            span_label: "GlobalDeny",
            reason,
        }),
        FinalDecision::Block { reason, .. } => Some(GlobalStop {
            verb_upper: "BLOCK",
            verb_lower: "block",
            span_label: "GlobalBlock",
            reason,
        }),
        _ => None,
    }
}

/// Record the exit reason for a global stop and finalize the global phase.
pub(super) fn record_global_exit(
    telemetry: &mut Option<&mut TelemetryContext>,
    stop: &GlobalStop<'_>,
) {
    with_current_phase(telemetry, |phase| {
        phase
            .evaluation_mut()
            .record_exit(format!("Global {}: {}", stop.verb_lower, stop.reason));
        phase.finalize();
    });
}

#[cfg(test)]
mod tests {
    //! Tests for the global-stop classification.

    use super::*;

    fn stop_labels(decision: &FinalDecision) -> Option<(&'static str, &'static str, String)> {
        global_stop(decision).map(|s| (s.verb_upper, s.span_label, s.reason.to_string()))
    }

    #[test]
    fn halt_deny_and_block_stop_global_evaluation() {
        let halt = FinalDecision::Halt {
            reason: "h".into(),
            agent_messages: vec![],
        };
        let deny = FinalDecision::Deny {
            reason: "d".into(),
            agent_messages: vec![],
        };
        let block = FinalDecision::Block {
            reason: "b".into(),
            agent_messages: vec![],
        };
        assert_eq!(
            stop_labels(&halt),
            Some(("HALT", "GlobalHalt", "h".to_string()))
        );
        assert_eq!(
            stop_labels(&deny),
            Some(("DENY", "GlobalDeny", "d".to_string()))
        );
        assert_eq!(
            stop_labels(&block),
            Some(("BLOCK", "GlobalBlock", "b".to_string()))
        );
    }

    #[test]
    fn allow_does_not_stop_global_evaluation() {
        assert!(global_stop(&FinalDecision::Allow { context: vec![] }).is_none());
    }
}
