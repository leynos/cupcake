//! The enrich, policy, signals and evaluation phases of a cupcake span.

use super::*;

// ============================================================================
// Enrich Phase
// ============================================================================

/// Preprocessing phase - transforms raw event before policy evaluation.
///
/// Operations include:
/// - `whitespace_normalization` - Collapse multiple spaces (TOB-3 defense)
/// - `symlink_resolution` - Resolve symlinks to canonical paths (TOB-4 defense)
/// - `content_unification` - Unify Write/Edit content fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichPhase {
    /// Unique span identifier
    pub span_id: String,

    /// Parent span ID (CupcakeSpan.span_id)
    pub parent_span_id: String,

    /// Start time in nanoseconds since Unix epoch
    pub start_time_unix_nano: u64,

    /// End time in nanoseconds since Unix epoch
    pub end_time_unix_nano: u64,

    /// The event after preprocessing transformations
    pub enriched_event: Value,

    /// List of preprocessing operations applied
    pub operations: Vec<String>,

    /// Duration in microseconds
    pub duration_us: u64,
}

impl EnrichPhase {
    /// Create a new enrich phase.
    pub fn new(
        enriched_event: Value,
        operations: Vec<String>,
        duration_us: u64,
        parent_span_id: String,
        start_time_unix_nano: u64,
    ) -> Self {
        Self {
            span_id: generate_span_id(),
            parent_span_id,
            start_time_unix_nano,
            end_time_unix_nano: start_time_unix_nano + (duration_us * 1000),
            enriched_event,
            operations,
            duration_us,
        }
    }
}

// ============================================================================
// Policy Phase
// ============================================================================

/// A single policy evaluation phase (global or project).
///
/// Each phase follows the flow: route → collect signals → evaluate WASM → synthesize
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyPhase {
    /// Unique span identifier
    pub span_id: String,

    /// Parent span ID (CupcakeSpan.span_id)
    pub parent_span_id: String,

    /// Start time in nanoseconds since Unix epoch
    pub start_time_unix_nano: u64,

    /// End time in nanoseconds since Unix epoch
    pub end_time_unix_nano: u64,

    /// Phase name: "global" or "project"
    pub name: String,

    /// Signal collection sub-phase
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signals: Option<SignalsPhase>,

    /// Policy evaluation result
    pub evaluation: EvaluationResult,

    /// Total phase duration in milliseconds
    pub duration_ms: u64,

    // --- Internal timing (not serialized) ---
    #[serde(skip)]
    start_instant: Option<Instant>,
}

impl PolicyPhase {
    /// Create a new policy phase.
    pub fn new(name: impl Into<String>, parent_span_id: String) -> Self {
        Self {
            span_id: generate_span_id(),
            parent_span_id,
            start_time_unix_nano: system_time_to_nanos(&SystemTime::now()),
            end_time_unix_nano: 0,
            name: name.into(),
            signals: None,
            evaluation: EvaluationResult::default(),
            duration_ms: 0,
            start_instant: Some(Instant::now()),
        }
    }

    /// Get the span_id for child spans.
    pub fn span_id(&self) -> &str {
        &self.span_id
    }

    /// Set the signals phase.
    pub fn set_signals(&mut self, signals: SignalsPhase) {
        self.signals = Some(signals);
    }

    /// Get mutable reference to evaluation result.
    pub fn evaluation_mut(&mut self) -> &mut EvaluationResult {
        &mut self.evaluation
    }

    /// Record a signal execution.
    /// Creates the SignalsPhase if it doesn't exist.
    pub fn record_signal(&mut self, signal: SignalExecution) {
        if self.signals.is_none() {
            self.signals = Some(SignalsPhase::new(
                self.span_id.clone(),
                self.start_time_unix_nano,
            ));
        }
        if let Some(ref mut signals) = self.signals {
            signals.add_signal(signal);
        }
    }

    /// Finalize the phase, recording duration.
    pub fn finalize(&mut self) {
        self.end_time_unix_nano = system_time_to_nanos(&SystemTime::now());
        if let Some(start) = self.start_instant.take() {
            self.duration_ms = start.elapsed().as_millis() as u64;
        }
    }
}

// ============================================================================
// Signals Phase
// ============================================================================

/// Signal collection phase - executes external programs to gather context.
///
/// Signals are shell commands defined in the rulebook that provide additional
/// context for policy evaluation (e.g., `git status`, file checks).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalsPhase {
    /// Unique span identifier
    pub span_id: String,

    /// Parent span ID (PolicyPhase.span_id)
    pub parent_span_id: String,

    /// Start time in nanoseconds since Unix epoch
    pub start_time_unix_nano: u64,

    /// End time in nanoseconds since Unix epoch
    pub end_time_unix_nano: u64,

    /// Individual signal executions
    pub signals: Vec<SignalExecution>,

    /// Total signal collection duration in milliseconds
    pub duration_ms: u64,
}

impl SignalsPhase {
    /// Create a new signals phase.
    pub fn new(parent_span_id: String, start_time_unix_nano: u64) -> Self {
        Self {
            span_id: generate_span_id(),
            parent_span_id,
            start_time_unix_nano,
            end_time_unix_nano: 0,
            signals: Vec::new(),
            duration_ms: 0,
        }
    }

    /// Add a signal execution.
    pub fn add_signal(&mut self, signal: SignalExecution) {
        self.signals.push(signal);
    }

    /// Finalize with end time and duration.
    pub fn finalize(&mut self, duration_ms: u64) {
        self.end_time_unix_nano = system_time_to_nanos(&SystemTime::now());
        self.duration_ms = duration_ms;
    }
}

/// Record of a single signal execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalExecution {
    /// Signal name from rulebook
    pub name: String,

    /// Shell command that was executed
    pub command: String,

    /// Result (parsed JSON or raw output)
    pub result: Value,

    /// Execution duration in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,

    /// Exit code if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

// ============================================================================
// Evaluation Result
// ============================================================================

/// Result of policy evaluation (WASM execution + synthesis).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvaluationResult {
    /// Whether routing found matching policies
    pub routed: bool,

    /// Names of policies that matched during routing
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_policies: Vec<String>,

    /// Raw decision set from WASM evaluation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wasm_decision_set: Option<DecisionSet>,

    /// Final synthesized decision
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_decision: Option<FinalDecision>,

    /// Reason for early exit (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_reason: Option<String>,
}

impl EvaluationResult {
    /// Record routing results.
    pub fn record_routing(&mut self, matched: bool, policies: &[String]) {
        self.routed = matched;
        self.matched_policies = policies.to_vec();
    }

    /// Record WASM evaluation results.
    pub fn record_wasm_result(&mut self, decision_set: &DecisionSet) {
        self.wasm_decision_set = Some(decision_set.clone());
    }

    /// Record the final synthesized decision.
    pub fn record_final_decision(&mut self, decision: &FinalDecision) {
        self.final_decision = Some(decision.clone());
    }

    /// Record an early exit reason.
    pub fn record_exit(&mut self, reason: impl Into<String>) {
        self.exit_reason = Some(reason.into());
    }
}
