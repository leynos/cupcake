//! Tests for merging builtin-generated signals into a rulebook.

use super::*;

fn signal(command: &str) -> SignalConfig {
    SignalConfig {
        command: command.to_string(),
        timeout_seconds: 5,
    }
}

#[test]
fn merge_builtin_signals_adds_new_signals_and_keeps_user_defined_ones() {
    let mut rulebook = Rulebook::default();
    rulebook
        .signals
        .insert("shared".to_string(), signal("user-command"));

    rulebook.merge_builtin_signals(HashMap::from([
        ("shared".to_string(), signal("builtin-command")),
        ("fresh".to_string(), signal("builtin-fresh")),
    ]));

    assert_eq!(rulebook.signals.len(), 2);
    assert_eq!(rulebook.signals["shared"].command, "user-command");
    assert_eq!(rulebook.signals["fresh"].command, "builtin-fresh");
}
