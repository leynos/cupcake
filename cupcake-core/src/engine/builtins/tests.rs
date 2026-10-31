//! Tests for builtin configuration, validation and signal generation.

use super::*;

#[test]
fn test_builtins_config_parsing() {
    // Test with explicit enabled values
    let yaml = r#"
claude_code_always_inject_on_prompt:
  enabled: true
  context:
    - "Test context"

git_block_no_verify:
  enabled: false
  message: "No bypassing hooks"
"#;

    let config: BuiltinsConfig = serde_yaml_ng::from_str(yaml).unwrap();

    assert!(
        config
            .claude_code_always_inject_on_prompt
            .as_ref()
            .unwrap()
            .enabled
    );
    assert!(!config.git_block_no_verify.as_ref().unwrap().enabled);
    assert_eq!(
        config.enabled_builtins(),
        vec!["claude_code_always_inject_on_prompt"]
    );
}

#[test]
fn test_default_enabled() {
    // Test that builtins default to DISABLED when field is omitted
    // This ensures builtins must be explicitly enabled with `enabled: true`
    let yaml = r#"
claude_code_always_inject_on_prompt:
  context:
    - "Test context"

git_pre_check:
  checks:
    - command: "cargo test"
      message: "Tests must pass"
"#;

    let config: BuiltinsConfig = serde_yaml_ng::from_str(yaml).unwrap();

    // Both should default to enabled=false when not explicitly set
    assert!(
        !config
            .claude_code_always_inject_on_prompt
            .as_ref()
            .unwrap()
            .enabled,
        "Builtins should default to disabled"
    );
    assert!(
        !config.git_pre_check.as_ref().unwrap().enabled,
        "Builtins should default to disabled"
    );

    let enabled = config.enabled_builtins();
    assert!(
        !enabled.contains(&"claude_code_always_inject_on_prompt".to_string()),
        "Disabled builtins should not appear in enabled list"
    );
    assert!(
        !enabled.contains(&"git_pre_check".to_string()),
        "Disabled builtins should not appear in enabled list"
    );
}

#[test]
fn test_validation() {
    // Test valid configuration passes
    let mut config = BuiltinsConfig::default();
    assert!(config.validate().is_ok());

    // Test empty enabled builtin fails
    config.claude_code_always_inject_on_prompt = Some(AlwaysInjectConfig {
        enabled: true,
        context: vec![],
    });
    let result = config.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors[0].contains("no context configured"));

    // Test invalid dynamic source
    config.claude_code_always_inject_on_prompt = Some(AlwaysInjectConfig {
        enabled: true,
        context: vec![ContextSource::Dynamic {
            file: None,
            command: None,
        }],
    });
    let result = config.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| e.contains("must have either 'file' or 'command'")));

    // Test extension with dot fails
    let mut by_ext = HashMap::new();
    by_ext.insert(
        ".rs".to_string(),
        CheckConfig {
            command: "cargo check".to_string(),
            message: "Check Rust".to_string(),
        },
    );
    config.post_edit_check = Some(PostEditCheckConfig {
        enabled: true,
        by_extension: by_ext,
    });
    let result = config.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("should not include dot")));

    // Test valid configuration
    let valid_config = BuiltinsConfig {
        git_pre_check: Some(GitPreCheckConfig {
            enabled: true,
            checks: vec![CheckConfig {
                command: "cargo test".to_string(),
                message: "Run tests".to_string(),
            }],
        }),
        ..Default::default()
    };
    assert!(valid_config.validate().is_ok());
}

#[test]
fn test_signal_generation() {
    let mut config = BuiltinsConfig::default();

    // Empty config doesn't generate signals
    let signals = config.generate_signals();
    assert_eq!(signals.len(), 0);

    // Add git_pre_check
    config.git_pre_check = Some(GitPreCheckConfig {
        enabled: true,
        checks: vec![CheckConfig {
            command: "cargo test".to_string(),
            message: "Tests must pass".to_string(),
        }],
    });

    let signals = config.generate_signals();
    assert_eq!(signals.len(), 1);
    assert!(signals.contains_key("__builtin_git_check_0"));
}
