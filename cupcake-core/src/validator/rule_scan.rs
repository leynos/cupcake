//! Line scans shared by the policy validation rules.

use super::decision_event_matrix::DecisionVerb;
use super::PolicyContent;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;

static RULE_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)\s+contains\s+").unwrap());

static VERB_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_]*)\s+contains\s+").unwrap());

/// Map each rule name to the (zero-based) lines on which it is defined.
pub(super) fn collect_rule_locations(policy: &PolicyContent) -> HashMap<String, Vec<usize>> {
    let mut rule_locations: HashMap<String, Vec<usize>> = HashMap::new();

    for (i, line) in policy.lines.iter().enumerate() {
        let trimmed = line.trim();
        let rule_name = RULE_PATTERN.captures(trimmed).and_then(|c| c.get(1));
        if let Some(rule_name) = rule_name {
            rule_locations
                .entry(rule_name.as_str().to_string())
                .or_default()
                .push(i);
        }
    }

    rule_locations
}

/// Whether the sorted definitions of `rule_name` have no different rule between them.
///
/// Comments and blank lines between definitions do not count as a gap.
pub(super) fn is_rule_grouped(
    policy: &PolicyContent,
    rule_name: &str,
    sorted_locations: &[usize],
) -> bool {
    sorted_locations
        .windows(2)
        .all(|window| !has_other_rule_between(policy, rule_name, window[0] + 1..window[1]))
}

/// Whether any line in `gap` defines a rule other than `rule_name`.
fn has_other_rule_between(
    policy: &PolicyContent,
    rule_name: &str,
    gap: std::ops::Range<usize>,
) -> bool {
    policy.lines[gap].iter().any(|line| {
        RULE_PATTERN
            .captures(line.trim())
            .and_then(|c| c.get(1))
            .is_some_and(|other_rule| other_rule.as_str() != rule_name)
    })
}

/// Find every decision verb used in the policy, with the lines that use it.
pub(super) fn collect_decision_verbs(policy: &PolicyContent) -> HashMap<DecisionVerb, Vec<usize>> {
    let mut found_verbs: HashMap<DecisionVerb, Vec<usize>> = HashMap::new();

    for (i, line) in policy.lines.iter().enumerate() {
        let verb = VERB_PATTERN
            .captures(line.trim())
            .and_then(|c| c.get(1))
            .and_then(|verb_name| DecisionVerb::from_rego_name(verb_name.as_str()));
        if let Some(verb) = verb {
            found_verbs.entry(verb).or_default().push(i);
        }
    }

    found_verbs
}
