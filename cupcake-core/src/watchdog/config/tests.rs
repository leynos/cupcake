//! Tests for watchdog configuration parsing and rules context loading.

use super::*;

#[test]
fn test_default_config() {
    let config = WatchdogConfig::default();
    assert!(!config.enabled);
    assert_eq!(config.backend, "openrouter");
    assert_eq!(config.timeout_seconds, 10);
    assert!(config.allows_on_error());
}

#[test]
fn test_yaml_parsing() {
    let yaml = r#"
enabled: true
backend: openrouter
timeout_seconds: 15
on_error: deny
openrouter:
  model: "google/gemini-2.5-flash"
  api_key_env: "MY_API_KEY"
"#;

    let config: WatchdogConfig = serde_yaml_ng::from_str(yaml).unwrap();
    assert!(config.enabled);
    assert_eq!(config.timeout_seconds, 15);
    assert!(!config.allows_on_error());

    let or = config.openrouter.unwrap();
    assert_eq!(or.model, "google/gemini-2.5-flash");
    assert_eq!(or.api_key_env, "MY_API_KEY");
}

#[test]
fn test_is_usable() {
    let mut config = WatchdogConfig::default();
    assert!(!config.is_usable()); // Not enabled

    config.enabled = true;
    assert!(config.is_usable()); // Now usable (openrouter is default backend)

    config.backend = "unknown".to_string();
    assert!(!config.is_usable()); // Unknown backend
}

#[test]
fn test_dir_config_json_parsing() {
    let json = r#"{
            "backend": "openrouter",
            "model": "google/gemini-2.5-flash",
            "timeout_seconds": 15,
            "on_error": "deny",
            "api_key_env": "MY_KEY"
        }"#;

    let dir_config: WatchdogDirConfig = serde_json::from_str(json).unwrap();
    assert_eq!(dir_config.backend, "openrouter");
    assert_eq!(dir_config.model, "google/gemini-2.5-flash");
    assert_eq!(dir_config.timeout_seconds, 15);
    assert_eq!(dir_config.on_error, "deny");
    assert_eq!(dir_config.api_key_env, "MY_KEY");

    let config = dir_config.into_watchdog_config();
    assert!(config.enabled);
    assert!(!config.allows_on_error());
}

#[test]
fn test_dir_config_defaults() {
    let json = "{}";
    let dir_config: WatchdogDirConfig = serde_json::from_str(json).unwrap();

    assert_eq!(dir_config.backend, "openrouter");
    assert_eq!(dir_config.model, "google/gemini-2.5-flash");
    assert_eq!(dir_config.timeout_seconds, 10);
    assert_eq!(dir_config.on_error, "allow");
    assert_eq!(dir_config.api_key_env, "OPENROUTER_API_KEY");
}

#[test]
fn test_effective_openrouter_config() {
    let config = WatchdogConfig {
        enabled: true,
        timeout_seconds: 20,
        openrouter: Some(OpenRouterConfig {
            model: "test-model".to_string(),
            ..Default::default()
        }),
        ..Default::default()
    };

    let effective = config.effective_openrouter_config();
    assert_eq!(effective.timeout_seconds, 20); // Inherited
    assert_eq!(effective.model, "test-model");
}

#[test]
fn test_shorthand_true() {
    // Simulate `watchdog: true` in YAML
    let input: WatchdogConfigInput = serde_yaml_ng::from_str("true").unwrap();
    let config: WatchdogConfig = input.into();

    assert!(config.enabled);
    assert_eq!(config.backend, "openrouter");
    assert!(config.openrouter.is_some());
}

#[test]
fn test_shorthand_false() {
    // Simulate `watchdog: false` in YAML
    let input: WatchdogConfigInput = serde_yaml_ng::from_str("false").unwrap();
    let config: WatchdogConfig = input.into();

    assert!(!config.enabled);
}

#[test]
fn test_shorthand_full_object() {
    // Simulate full config object in YAML
    let yaml = r#"
enabled: true
backend: openrouter
timeout_seconds: 15
on_error: deny
"#;
    let input: WatchdogConfigInput = serde_yaml_ng::from_str(yaml).unwrap();
    let config: WatchdogConfig = input.into();

    assert!(config.enabled);
    assert_eq!(config.timeout_seconds, 15);
    assert!(!config.allows_on_error());
}

#[test]
fn test_rulebook_shorthand_parsing() {
    // Test that rulebook can parse `watchdog: true`
    // We test the WatchdogConfigInput directly since it's what the
    // custom deserializer in rulebook.rs uses

    // Shorthand true
    let yaml = "true";
    let input: WatchdogConfigInput = serde_yaml_ng::from_str(yaml).unwrap();
    let config: WatchdogConfig = input.into();
    assert!(config.enabled);
    assert!(config.openrouter.is_some());

    // Full object
    let yaml = "enabled: true\nbackend: openrouter";
    let input: WatchdogConfigInput = serde_yaml_ng::from_str(yaml).unwrap();
    let config: WatchdogConfig = input.into();
    assert!(config.enabled);
    assert_eq!(config.backend, "openrouter");
}
