//! Telemetry span types for cupcake event lifecycle.
//!
//! The trace hierarchy follows the natural flow of policy evaluation:
//!
//! ```text
//! CupcakeSpan (root - entire evaluation lifecycle)
//!   ├── raw_event, harness, trace_id
//!   │
//!   ├── enrich: EnrichPhase (preprocessing)
//!   │     └── operations, enriched_event, duration
//!   │
//!   ├── phases: Vec<PolicyPhase> (one per policy layer)
//!   │     ├── name: "global" | "project"
//!   │     ├── signals: SignalsPhase (external program collection)
//!   │     │     └── signals: Vec<SignalExecution>
//!   │     └── evaluation: EvaluationResult (WASM → decision)
//!   │
//!   └── response, total_duration_ms
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::engine::decision::{DecisionSet, FinalDecision};
use crate::harness::types::HarnessType;

// ============================================================================
// Helper Functions
// ============================================================================

/// Generate a unique 16-character hex span ID (8 bytes).
fn generate_span_id() -> String {
    let uuid = Uuid::now_v7();
    hex::encode(&uuid.as_bytes()[8..16])
}

/// Convert SystemTime to nanoseconds since Unix epoch.
fn system_time_to_nanos(time: &SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// Serialize SystemTime as RFC3339 string.
fn serialize_system_time<S>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use chrono::{DateTime, Utc};
    let datetime: DateTime<Utc> = (*time).into();
    serializer.serialize_str(&datetime.to_rfc3339())
}

// ============================================================================
// Root Span
// ============================================================================

/// Root span capturing the entire cupcake evaluation lifecycle.
///
/// Created immediately when an agent event arrives, before any processing.
/// Contains all child phases and the final response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CupcakeSpan {
    // --- OTLP-compatible span identity ---
    /// Unique span identifier (16-char hex)
    pub span_id: String,

    /// Trace identifier for correlation (UUID v7, time-ordered)
    pub trace_id: String,

    /// Start time in nanoseconds since Unix epoch
    pub start_time_unix_nano: u64,

    /// End time in nanoseconds since Unix epoch (set at finalize)
    pub end_time_unix_nano: u64,

    // --- Event metadata ---
    /// The raw event exactly as received from the agent
    pub raw_event: Value,

    /// Which harness (agent) sent this event
    pub harness: HarnessType,

    /// When the event was received (human-readable)
    #[serde(serialize_with = "serialize_system_time")]
    pub timestamp: SystemTime,

    // --- Processing phases ---
    /// Preprocessing phase (symlink resolution, whitespace normalization)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enrich: Option<EnrichPhase>,

    /// Policy evaluation phases (global → project)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub phases: Vec<PolicyPhase>,

    // --- Final output ---
    /// Response sent back to the agent
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<Value>,

    /// Errors encountered during processing
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,

    /// Total evaluation duration in milliseconds
    pub total_duration_ms: u64,

    // --- Internal timing (not serialized) ---
    #[serde(skip)]
    start_instant: Option<Instant>,
}

impl CupcakeSpan {
    /// Create a new root span capturing the raw agent event.
    pub fn new(raw_event: Value, harness: HarnessType, trace_id: String) -> Self {
        let now = SystemTime::now();
        Self {
            span_id: generate_span_id(),
            trace_id,
            start_time_unix_nano: system_time_to_nanos(&now),
            end_time_unix_nano: 0,
            raw_event,
            harness,
            timestamp: now,
            enrich: None,
            phases: Vec::new(),
            response: None,
            errors: Vec::new(),
            total_duration_ms: 0,
            start_instant: Some(Instant::now()),
        }
    }

    /// Get the span_id for child spans to reference as parent.
    pub fn span_id(&self) -> &str {
        &self.span_id
    }

    /// Record the enrichment phase.
    pub fn set_enrich(&mut self, enrich: EnrichPhase) {
        self.enrich = Some(enrich);
    }

    /// Add a policy evaluation phase.
    pub fn add_phase(&mut self, phase: PolicyPhase) {
        self.phases.push(phase);
    }

    /// Get the current (last) policy phase mutably.
    pub fn current_phase_mut(&mut self) -> Option<&mut PolicyPhase> {
        self.phases.last_mut()
    }

    /// Record an error.
    pub fn add_error(&mut self, error: impl Into<String>) {
        self.errors.push(error.into());
    }

    /// Finalize the span with the response.
    pub fn finalize(&mut self, response: Option<Value>) {
        self.response = response;
        self.end_time_unix_nano = system_time_to_nanos(&SystemTime::now());
        if let Some(start) = self.start_instant.take() {
            self.total_duration_ms = start.elapsed().as_millis() as u64;
        }
    }
}

mod phases;
#[cfg(test)]
mod tests;

pub use phases::*;
