//! Normalization implementations for various input patterns
//!
//! This module contains the actual normalization logic for different types
//! of adversarial patterns. Each normalizer focuses on a specific pattern
//! type and can be applied independently.

use tracing::trace;

/// Normalizer for whitespace-based obfuscation patterns
///
/// Handles:
/// - Multiple consecutive spaces
/// - Tabs converted to spaces
/// - Leading/trailing whitespace
/// - Preserves quoted strings
pub struct WhitespaceNormalizer;

impl WhitespaceNormalizer {
    /// Normalize a shell command string while preserving quoted content
    ///
    /// # Rules
    /// 1. Collapse consecutive whitespace to single space (outside quotes)
    /// 2. Convert tabs/newlines to spaces
    /// 3. Trim leading/trailing whitespace
    /// 4. Preserve content within quotes exactly
    ///
    /// # Examples
    /// ```
    /// # use cupcake_core::preprocessing::normalizers::WhitespaceNormalizer;
    /// assert_eq!(WhitespaceNormalizer::normalize_command("rm  -rf  test"), "rm -rf test");
    /// assert_eq!(WhitespaceNormalizer::normalize_command("echo 'test  test'"), "echo 'test  test'");
    /// assert_eq!(WhitespaceNormalizer::normalize_command("echo \"multi  space\""), "echo \"multi  space\"");
    /// ```
    pub fn normalize_command(command: &str) -> String {
        let mut result = String::with_capacity(command.len());
        let mut chars = command.chars().peekable();
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        let mut last_was_space = false;
        let mut escape_next = false;

        for ch in chars.by_ref() {
            // Handle escape sequences
            if escape_next {
                result.push(ch);
                escape_next = false;
                last_was_space = false;
                continue;
            }

            // Check for escape character
            if ch == '\\' && !in_single_quote {
                result.push(ch);
                escape_next = true;
                last_was_space = false;
                continue;
            }

            // Handle quotes
            if ch == '\'' && !in_double_quote {
                in_single_quote = !in_single_quote;
                result.push(ch);
                last_was_space = false;
                continue;
            }

            if ch == '"' && !in_single_quote {
                in_double_quote = !in_double_quote;
                result.push(ch);
                last_was_space = false;
                continue;
            }

            // Inside quotes: preserve everything exactly
            if in_single_quote || in_double_quote {
                result.push(ch);
                last_was_space = false;
                continue;
            }

            // Outside quotes: normalize whitespace
            if ch.is_whitespace() {
                if !last_was_space {
                    result.push(' '); // Normalize all whitespace to single space
                    last_was_space = true;
                }
                // Consecutive whitespace is implicitly skipped when last_was_space is true
            } else {
                result.push(ch);
                last_was_space = false;
            }
        }

        // Trim the result
        let trimmed = result.trim();

        trace!("Normalized command: '{}' → '{}'", command, trimmed);
        trimmed.to_string()
    }

    /// Check if normalization would change the command
    ///
    /// Useful for avoiding unnecessary allocations and logging
    pub fn would_normalize(command: &str) -> bool {
        // Quick checks that avoid full parsing
        if command.starts_with(' ') || command.ends_with(' ') {
            return true;
        }

        // Check for consecutive whitespace or tabs
        let mut prev_was_space = false;
        let mut quotes = QuoteState::default();

        for ch in command.chars() {
            // Quoting syntax and quoted text are never normalized
            if quotes.consume_syntax(ch) || quotes.is_quoted() {
                prev_was_space = false;
                continue;
            }

            if matches!(ch, '\t' | '\n' | '\r') {
                return true; // Contains tab or newline
            }

            if ch == ' ' {
                if prev_was_space {
                    return true; // Consecutive spaces
                }
                prev_was_space = true;
            } else {
                prev_was_space = false;
            }
        }

        false
    }
}

/// Quote and escape state while scanning a shell command.
#[derive(Default)]
struct QuoteState {
    in_single_quote: bool,
    in_double_quote: bool,
    escape_next: bool,
}

impl QuoteState {
    /// Update the state for `ch`; returns true when `ch` is escaped or is escape or quote syntax.
    fn consume_syntax(&mut self, ch: char) -> bool {
        if self.escape_next {
            self.escape_next = false;
            return true;
        }
        if ch == '\\' && !self.in_single_quote {
            self.escape_next = true;
            return true;
        }
        if ch == '\'' && !self.in_double_quote {
            self.in_single_quote = !self.in_single_quote;
            return true;
        }
        if ch == '"' && !self.in_single_quote {
            self.in_double_quote = !self.in_double_quote;
            return true;
        }
        false
    }

    /// True while inside single or double quotes.
    fn is_quoted(&self) -> bool {
        self.in_single_quote || self.in_double_quote
    }
}

#[cfg(test)]
mod tests {
    //! Tests for the `normalizers` module.

    use super::*;

    #[test]
    fn test_basic_whitespace_normalization() {
        // Multiple spaces
        assert_eq!(WhitespaceNormalizer::normalize_command("rm  -rf"), "rm -rf");
        assert_eq!(
            WhitespaceNormalizer::normalize_command("rm   -rf   test"),
            "rm -rf test"
        );

        // Leading/trailing spaces
        assert_eq!(
            WhitespaceNormalizer::normalize_command("  rm -rf  "),
            "rm -rf"
        );
        assert_eq!(
            WhitespaceNormalizer::normalize_command("   ls -la   "),
            "ls -la"
        );

        // Tabs
        assert_eq!(WhitespaceNormalizer::normalize_command("rm\t-rf"), "rm -rf");
        assert_eq!(
            WhitespaceNormalizer::normalize_command("rm\t\t-rf"),
            "rm -rf"
        );

        // Newlines
        assert_eq!(WhitespaceNormalizer::normalize_command("rm\n-rf"), "rm -rf");

        // Mixed whitespace
        assert_eq!(
            WhitespaceNormalizer::normalize_command("rm  \t  -rf"),
            "rm -rf"
        );
    }

    #[test]
    fn test_quote_preservation() {
        // Single quotes
        assert_eq!(
            WhitespaceNormalizer::normalize_command("echo 'test  test'"),
            "echo 'test  test'"
        );
        assert_eq!(
            WhitespaceNormalizer::normalize_command("echo  'multiple  spaces'"),
            "echo 'multiple  spaces'"
        );

        // Double quotes
        assert_eq!(
            WhitespaceNormalizer::normalize_command("echo \"test  test\""),
            "echo \"test  test\""
        );
        assert_eq!(
            WhitespaceNormalizer::normalize_command("echo  \"tabs\tin\there\""),
            "echo \"tabs\tin\there\""
        );

        // Mixed quotes
        assert_eq!(
            WhitespaceNormalizer::normalize_command("echo  'single'  \"double\""),
            "echo 'single' \"double\""
        );
    }

    #[test]
    fn test_escaped_quotes() {
        // Escaped single quote
        assert_eq!(
            WhitespaceNormalizer::normalize_command("echo  \\'test  test\\'"),
            "echo \\'test test\\'"
        );

        // Escaped double quote
        assert_eq!(
            WhitespaceNormalizer::normalize_command("echo  \\\"test  test\\\""),
            "echo \\\"test test\\\""
        );

        // Escaped backslash
        assert_eq!(
            WhitespaceNormalizer::normalize_command("echo  \\\\  test"),
            "echo \\\\ test"
        );
    }

    #[test]
    fn test_complex_commands() {
        // Command with pipes
        assert_eq!(
            WhitespaceNormalizer::normalize_command("ls  -la  |  grep  test"),
            "ls -la | grep test"
        );

        // Command with redirects
        assert_eq!(
            WhitespaceNormalizer::normalize_command("echo  test  >  file.txt"),
            "echo test > file.txt"
        );

        // Command with variables
        assert_eq!(
            WhitespaceNormalizer::normalize_command("echo  $HOME  $USER"),
            "echo $HOME $USER"
        );

        // Real-world example
        assert_eq!(
            WhitespaceNormalizer::normalize_command("git  commit  -m  'Fix  spacing  issue'"),
            "git commit -m 'Fix  spacing  issue'" // Spaces in message preserved
        );
    }

    #[test]
    fn test_would_normalize() {
        // Should normalize
        assert!(WhitespaceNormalizer::would_normalize("rm  -rf"));
        assert!(WhitespaceNormalizer::would_normalize("  rm -rf"));
        assert!(WhitespaceNormalizer::would_normalize("rm -rf  "));
        assert!(WhitespaceNormalizer::would_normalize("rm\t-rf"));

        // Should not normalize
        assert!(!WhitespaceNormalizer::would_normalize("rm -rf"));
        assert!(!WhitespaceNormalizer::would_normalize("echo 'test  test'"));
        assert!(!WhitespaceNormalizer::would_normalize("ls -la"));
    }

    #[test]
    fn test_would_normalize_respects_quotes_and_escapes() {
        // Double-quoted, escaped and mixed-quote runs are preserved, not normalized
        assert!(!WhitespaceNormalizer::would_normalize("echo \"a  b\""));
        assert!(!WhitespaceNormalizer::would_normalize("echo a\\  b"));
        assert!(!WhitespaceNormalizer::would_normalize("echo \"it's  ok\""));
        assert!(!WhitespaceNormalizer::would_normalize(
            "echo 'say \"a  b\"'"
        ));
        // Whitespace after the closing quote is outside the quotes again
        assert!(WhitespaceNormalizer::would_normalize("echo 'a'  b"));
        assert!(WhitespaceNormalizer::would_normalize("echo \"a\"\tb"));
        // A backslash is literal inside single quotes, so the quote still closes
        assert!(WhitespaceNormalizer::would_normalize("echo 'a\\'  b"));
    }

    #[test]
    fn test_no_change_commands() {
        // Commands that should not change
        let commands = vec![
            "rm -rf test",
            "ls -la",
            "git commit -m 'message'",
            "echo 'spaces  inside'",
            "cat file.txt",
        ];

        for cmd in commands {
            assert_eq!(WhitespaceNormalizer::normalize_command(cmd), cmd);
        }
    }

    #[test]
    fn test_adversarial_examples_from_tob() {
        // From TOB-EQTY-LAB-CUPCAKE-3 Exploit Scenario 1
        assert_eq!(
            WhitespaceNormalizer::normalize_command("rm  -rf /tmp/testdir"),
            "rm -rf /tmp/testdir"
        );

        // With more spaces
        assert_eq!(
            WhitespaceNormalizer::normalize_command("rm   -rf   /tmp/testdir"),
            "rm -rf /tmp/testdir"
        );

        // With tabs
        assert_eq!(
            WhitespaceNormalizer::normalize_command("rm\t-rf\t/tmp/testdir"),
            "rm -rf /tmp/testdir"
        );
    }
}
