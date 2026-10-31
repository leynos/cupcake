//! Configuration structures for each builtin abstraction.

use serde::{Deserialize, Serialize};

use super::*;

/// Configuration for all builtin abstractions
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuiltinsConfig {
    /// Always inject on prompt configuration (Claude Code only)
    #[serde(default, alias = "always_inject_on_prompt")]
    pub claude_code_always_inject_on_prompt: Option<AlwaysInjectConfig>,

    /// Git pre-check configuration
    #[serde(default)]
    pub git_pre_check: Option<GitPreCheckConfig>,

    /// Post-edit check configuration
    #[serde(default)]
    pub post_edit_check: Option<PostEditCheckConfig>,

    /// Rulebook security guardrails configuration
    #[serde(default)]
    pub rulebook_security_guardrails: Option<RulebookSecurityConfig>,

    /// Protected paths configuration (user-defined read-only paths)
    #[serde(default)]
    pub protected_paths: Option<ProtectedPathsConfig>,

    /// Git block no-verify configuration (prevents bypassing commit hooks)
    #[serde(default)]
    pub git_block_no_verify: Option<GitBlockNoVerifyConfig>,

    // Global-only builtins (for machine-wide security)
    /// System protection configuration - prevents modification of OS paths
    #[serde(default)]
    pub system_protection: Option<SystemProtectionConfig>,

    /// Sensitive data protection - blocks reading credentials/secrets
    #[serde(default)]
    pub sensitive_data_protection: Option<SensitiveDataProtectionConfig>,

    /// Cupcake execution protection - prevents direct binary execution
    #[serde(default)]
    pub cupcake_exec_protection: Option<CupcakeExecProtectionConfig>,

    /// Enforce full file read - prevents partial reads of small files (Claude Code only)
    #[serde(default, alias = "enforce_full_file_read")]
    pub claude_code_enforce_full_file_read: Option<EnforceFullFileReadConfig>,
}

/// Configuration for always_inject_on_prompt builtin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlwaysInjectConfig {
    /// Whether this builtin is enabled (defaults to true)
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Context sources to inject
    #[serde(default)]
    pub context: Vec<ContextSource>,
}

fn default_enabled() -> bool {
    false // Builtins must be explicitly enabled with `enabled: true`
}

/// Configuration for git_pre_check builtin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitPreCheckConfig {
    /// Whether this builtin is enabled (defaults to true)
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Checks to run before git operations
    #[serde(default)]
    pub checks: Vec<CheckConfig>,
}

/// Configuration for post_edit_check builtin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostEditCheckConfig {
    /// Whether this builtin is enabled (defaults to true)
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Checks by file extension
    #[serde(default)]
    pub by_extension: HashMap<String, CheckConfig>,
}

/// A check command with message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckConfig {
    /// Command to execute
    pub command: String,

    /// Message to display if check fails
    pub message: String,
}

/// Source of context data (string, file, or command)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContextSource {
    /// Static string context
    String(String),

    /// Context from file or command
    Dynamic {
        /// File path to read
        #[serde(default)]
        file: Option<String>,

        /// Command to execute
        #[serde(default)]
        command: Option<String>,
    },
}

/// Configuration for rulebook_security_guardrails builtin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulebookSecurityConfig {
    /// Whether this builtin is enabled (defaults to true)
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Message to show when blocking operations
    #[serde(default = "default_rulebook_security_message")]
    pub message: String,

    /// Protected paths (defaults to [".cupcake/"])
    #[serde(default = "default_protected_paths")]
    pub protected_paths: Vec<String>,
}

fn default_rulebook_security_message() -> String {
    "Cupcake configuration files are protected from modification".to_string()
}

fn default_protected_paths() -> Vec<String> {
    vec![".cupcake/".to_string()]
}

/// Configuration for protected_paths builtin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtectedPathsConfig {
    /// Whether this builtin is enabled (defaults to true)
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Message to show when blocking modifications
    #[serde(default = "default_protected_paths_message")]
    pub message: String,

    /// List of paths to protect (supports globs)
    #[serde(default)]
    pub paths: Vec<String>,
}

fn default_protected_paths_message() -> String {
    "This path is read-only and cannot be modified".to_string()
}

/// Configuration for git_block_no_verify builtin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitBlockNoVerifyConfig {
    /// Whether this builtin is enabled (defaults to true)
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Message to show when blocking operations
    #[serde(default = "default_git_block_no_verify_message")]
    pub message: String,

    /// Allow specific exceptions (e.g., for CI environments)
    #[serde(default)]
    pub exceptions: Vec<String>,
}

fn default_git_block_no_verify_message() -> String {
    "Git operations with --no-verify are not permitted. Commit hooks must run.".to_string()
}

// Global builtin configurations

/// Configuration for system protection builtin (global only)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemProtectionConfig {
    /// Whether this builtin is enabled (defaults to true)
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Additional custom paths to protect (beyond the defaults)
    #[serde(default)]
    pub additional_paths: Vec<String>,

    /// Custom message for blocked operations
    #[serde(default = "default_system_protection_message")]
    pub message: String,
}

fn default_system_protection_message() -> String {
    "Access to critical system path blocked".to_string()
}

/// Configuration for sensitive data protection builtin (global only)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitiveDataProtectionConfig {
    /// Whether this builtin is enabled (defaults to true)
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Additional file patterns to consider sensitive
    #[serde(default)]
    pub additional_patterns: Vec<String>,

    /// Custom message for blocked operations
    #[serde(default = "default_sensitive_data_message")]
    pub message: String,
}

fn default_sensitive_data_message() -> String {
    "Access to potentially sensitive data blocked".to_string()
}

/// Configuration for cupcake execution protection builtin (global only)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CupcakeExecProtectionConfig {
    /// Whether this builtin is enabled (defaults to true)
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Allow specific cupcake commands (e.g., ["version", "help"])
    #[serde(default)]
    pub allowed_commands: Vec<String>,

    /// Custom message for blocked operations
    #[serde(default = "default_cupcake_exec_message")]
    pub message: String,
}

fn default_cupcake_exec_message() -> String {
    "Direct execution of Cupcake binary is not permitted".to_string()
}

/// Configuration for enforce_full_file_read builtin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnforceFullFileReadConfig {
    /// Whether this builtin is enabled (defaults to true)
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Maximum lines threshold - files under this size must be read in full
    #[serde(default = "default_max_lines")]
    pub max_lines: usize,

    /// Message to show when blocking partial reads
    #[serde(default = "default_enforce_full_read_message")]
    pub message: String,
}

fn default_max_lines() -> usize {
    2000
}

fn default_enforce_full_read_message() -> String {
    "Please read the entire file first (files under 2000 lines must be read completely)".to_string()
}
