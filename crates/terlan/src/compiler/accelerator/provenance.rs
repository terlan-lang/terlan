//! Stable source locations for accelerator package diagnostics.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

/// One one-based location in an accelerator descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceleratorSourceSpan {
    /// Source path shown to the user.
    pub source: String,
    /// One-based source line.
    pub line: usize,
    /// One-based source column.
    pub column: usize,
}

impl AcceleratorSourceSpan {
    /// Returns the start of a source file for diagnostics without a narrower span.
    pub fn start(source: &Path) -> Self {
        Self {
            source: source.display().to_string(),
            line: 1,
            column: 1,
        }
    }
}

impl Default for AcceleratorSourceSpan {
    fn default() -> Self {
        Self {
            source: "<accelerator>".to_string(),
            line: 1,
            column: 1,
        }
    }
}

impl fmt::Display for AcceleratorSourceSpan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}:{}", self.source, self.line, self.column)
    }
}

/// Non-serialized source map associated with a parsed accelerator descriptor.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AcceleratorDescriptorSpans {
    /// Descriptor fallback location.
    descriptor: AcceleratorSourceSpan,
    /// Locations indexed by decoded scalar value.
    values: BTreeMap<String, Vec<AcceleratorSourceSpan>>,
    /// Locations indexed by assignment field and decoded scalar value.
    fields: BTreeMap<(String, String), Vec<AcceleratorSourceSpan>>,
}

impl AcceleratorDescriptorSpans {
    /// Scans parsed TOML text for diagnostic locations without interpreting its schema.
    pub fn scan(source: &str, path: &Path) -> Self {
        let mut spans = Self {
            descriptor: AcceleratorSourceSpan::start(path),
            values: BTreeMap::new(),
            fields: BTreeMap::new(),
        };
        let mut array_field: Option<String> = None;
        for (line_index, line) in source.lines().enumerate() {
            let assignment = line
                .split_once('=')
                .map(|(key, value)| (key.trim().to_string(), value.trim_start()));
            if let Some((field, value)) = &assignment {
                array_field = value.starts_with('[').then(|| field.clone());
                if !value.starts_with('[') {
                    record_bare_value(&mut spans, path, line, line_index, field, value);
                }
            }
            let active_field = assignment
                .as_ref()
                .map(|(field, _)| field.as_str())
                .or(array_field.as_deref());
            record_quoted_values(&mut spans, path, line, line_index, active_field);
            if array_field.is_some() && line.contains(']') {
                array_field = None;
            }
        }
        spans
    }

    /// Returns the descriptor fallback location.
    pub fn descriptor(&self) -> &AcceleratorSourceSpan {
        &self.descriptor
    }

    /// Returns the declaration location for a provided capability.
    pub fn capability(&self, capability: &str) -> &AcceleratorSourceSpan {
        self.field_value("capabilities", capability)
    }

    /// Returns the declaration location for a required capability.
    pub fn requirement(&self, capability: &str) -> &AcceleratorSourceSpan {
        self.field_value("requirements", capability)
    }

    /// Returns the declaration location for a target triple.
    pub fn target(&self, triple: &str) -> &AcceleratorSourceSpan {
        self.field_value("triple", triple)
    }

    /// Produces one stable compiler diagnostic at an explicit source location.
    pub fn diagnostic(
        &self,
        code: &str,
        span: &AcceleratorSourceSpan,
        message: String,
        fallback: &Path,
    ) -> String {
        let span = if span.source == "<accelerator>" {
            AcceleratorSourceSpan::start(fallback)
        } else {
            span.clone()
        };
        format!("error[accelerator.{code}]: {span}: {message}")
    }

    /// Adds a stable diagnostic code and the narrowest discoverable source span.
    pub fn decorate(&self, code: &str, message: &str, fallback: &Path) -> String {
        let values = backtick_values(message).collect::<Vec<_>>();
        let field_span = values.windows(2).find_map(|pair| {
            self.fields
                .get(&(pair[0].to_string(), pair[1].to_string()))
                .and_then(|spans| spans.last())
        });
        let span = field_span
            .or_else(|| {
                values
                    .iter()
                    .find_map(|value| self.values.get(*value).and_then(|spans| spans.last()))
            })
            .unwrap_or(&self.descriptor);
        let prefix = format!("{}: ", fallback.display());
        let message = message.strip_prefix(&prefix).unwrap_or(message);
        self.diagnostic(code, span, message.to_string(), fallback)
    }

    /// Looks up one field value and falls back to the descriptor start.
    fn field_value(&self, field: &str, value: &str) -> &AcceleratorSourceSpan {
        self.fields
            .get(&(field.to_string(), value.to_string()))
            .and_then(|spans| spans.last())
            .or_else(|| self.values.get(value).and_then(|spans| spans.last()))
            .unwrap_or(&self.descriptor)
    }

    /// Records one value in generic and field-specific indexes.
    fn record(&mut self, field: Option<&str>, value: String, span: AcceleratorSourceSpan) {
        self.values
            .entry(value.clone())
            .or_default()
            .push(span.clone());
        if let Some(field) = field {
            self.fields
                .entry((field.to_string(), value))
                .or_default()
                .push(span);
        }
    }
}

/// Records quoted TOML strings and their decoded field association.
fn record_quoted_values(
    spans: &mut AcceleratorDescriptorSpans,
    path: &Path,
    line: &str,
    line_index: usize,
    field: Option<&str>,
) {
    let bytes = line.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor] != b'"' {
            cursor += 1;
            continue;
        }
        let start = cursor;
        cursor += 1;
        let mut escaped = false;
        while cursor < bytes.len() {
            match bytes[cursor] {
                b'"' if !escaped => break,
                b'\\' if !escaped => escaped = true,
                _ => escaped = false,
            }
            cursor += 1;
        }
        if cursor >= bytes.len() {
            return;
        }
        let raw = &line[start + 1..cursor];
        let value = raw.replace("\\\"", "\"").replace("\\\\", "\\");
        let column = line[..start].chars().count() + 1;
        spans.record(
            field,
            value,
            AcceleratorSourceSpan {
                source: path.display().to_string(),
                line: line_index + 1,
                column,
            },
        );
        cursor += 1;
    }
}

/// Records a non-array scalar assignment such as a schema number.
fn record_bare_value(
    spans: &mut AcceleratorDescriptorSpans,
    path: &Path,
    line: &str,
    line_index: usize,
    field: &str,
    value: &str,
) {
    let value = value.split('#').next().unwrap_or(value).trim();
    if value.is_empty() || value.starts_with('"') {
        return;
    }
    let column = line.find(value).unwrap_or(0) + 1;
    spans.record(
        Some(field),
        value.to_string(),
        AcceleratorSourceSpan {
            source: path.display().to_string(),
            line: line_index + 1,
            column,
        },
    );
}

/// Iterates values enclosed in diagnostic backticks.
fn backtick_values(message: &str) -> impl Iterator<Item = &str> {
    message.split('`').skip(1).step_by(2)
}

#[cfg(test)]
#[path = "provenance_test.rs"]
mod provenance_test;
