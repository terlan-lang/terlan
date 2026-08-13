//! Regex adapter operations for `std.regex.Regex`.
//!
//! This module owns the concrete Rust implementation for Terlan's portable
//! regular-expression contract. It delegates parsing and matching to the
//! maintained `regex` crate and converts backend failures into stable
//! Terlan-facing errors.

use regex::Regex as RustRegex;

/// Compiled regular expression value owned by the VM/native adapter.
#[derive(Clone, Debug)]
pub struct Regex {
    pattern: String,
    value: RustRegex,
}

impl Regex {
    /// Builds a Regex wrapper from a pattern and compiled Rust regex.
    pub fn new(pattern: impl Into<String>, value: RustRegex) -> Self {
        Self {
            pattern: pattern.into(),
            value,
        }
    }

    /// Returns the original pattern text.
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// Returns the compiled Rust regex by shared reference.
    pub fn as_regex(&self) -> &RustRegex {
        &self.value
    }
}

impl PartialEq for Regex {
    fn eq(&self, other: &Self) -> bool {
        self.pattern == other.pattern
    }
}

impl Eq for Regex {}

/// Portable regex error returned by regex operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegexError {
    code: &'static str,
    message: String,
    offset: usize,
}

impl RegexError {
    /// Builds a portable regex error.
    pub fn new(code: &'static str, message: impl Into<String>, offset: usize) -> Self {
        Self {
            code,
            message: message.into(),
            offset,
        }
    }

    /// Returns the stable machine-readable error code.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Returns the human-readable diagnostic message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the byte offset associated with the regex error.
    pub fn offset(&self) -> usize {
        self.offset
    }
}

/// Compiles one regex pattern.
pub fn compile(pattern: &str) -> Result<Regex, RegexError> {
    RustRegex::new(pattern)
        .map(|value| Regex::new(pattern, value))
        .map_err(|error| RegexError::new("regex.compile", error.to_string(), 0))
}

/// Returns whether a compiled regex matches the text.
pub fn is_match(regex: &Regex, text: &str) -> bool {
    regex.as_regex().is_match(text)
}

/// Returns one-based source line numbers whose text matches the regex.
pub fn matching_line_numbers(regex: &Regex, text: &str) -> Vec<i64> {
    text.lines()
        .enumerate()
        .filter(|(_index, line)| regex.as_regex().is_match(line))
        .map(|(index, _line)| i64::try_from(index.saturating_add(1)).unwrap_or(i64::MAX))
        .collect()
}

/// Returns the first matched text, when present.
pub fn find(regex: &Regex, text: &str) -> Option<String> {
    regex
        .as_regex()
        .find(text)
        .map(|found| found.as_str().to_string())
}

/// Returns all non-overlapping matched text values.
pub fn find_all(regex: &Regex, text: &str) -> Vec<String> {
    regex
        .as_regex()
        .find_iter(text)
        .map(|found| found.as_str().to_string())
        .collect()
}

/// Returns one positional capture group, when present.
pub fn capture(regex: &Regex, text: &str, index: usize) -> Option<String> {
    regex
        .as_regex()
        .captures(text)
        .and_then(|captures| captures.get(index))
        .map(|found| found.as_str().to_string())
}

/// Returns one named capture group, when present.
pub fn named_capture(regex: &Regex, text: &str, name: &str) -> Option<String> {
    regex
        .as_regex()
        .captures(text)
        .and_then(|captures| captures.name(name))
        .map(|found| found.as_str().to_string())
}

/// Replaces all matches with the replacement text.
pub fn replace(regex: &Regex, text: &str, replacement: &str) -> String {
    regex.as_regex().replace_all(text, replacement).into_owned()
}

/// Splits text around all matches.
pub fn split(regex: &Regex, text: &str) -> Vec<String> {
    regex
        .as_regex()
        .split(text)
        .map(ToOwned::to_owned)
        .collect()
}

/// Escapes literal text so it can be used as a regex pattern.
pub fn escape(text: &str) -> String {
    regex::escape(text)
}

#[cfg(test)]
#[path = "regex_test.rs"]
#[cfg(test)]
mod regex_test;
