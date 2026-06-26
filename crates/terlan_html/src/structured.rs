use std::path::Path;

use serde_json::Value;
use yaml_rust::YamlLoader;

use crate::{
    artifact_template_target_from_path, parse_template, ArtifactTemplateTarget, HtmlDiagnostic,
};

/// Validates an artifact template's static structure by target suffix.
///
/// Inputs:
/// - `source`: template source text.
/// - `path`: source path whose suffix selects the target validator.
///
/// Output:
/// - `Ok(())` when the target validator accepts the source.
/// - `Err(Vec<HtmlDiagnostic>)` when the suffix is unknown, validation fails,
///   or the target validator is not implemented yet.
///
/// Transformation:
/// - Classifies the path with the shared artifact-template target contract and
///   delegates to the target-specific validator without rendering artifacts.
pub fn validate_artifact_template_structure(
    source: impl AsRef<str>,
    path: impl AsRef<Path>,
) -> Result<(), Vec<HtmlDiagnostic>> {
    let path = path.as_ref();
    let Some(target) = artifact_template_target_from_path(path) else {
        return Err(vec![HtmlDiagnostic::new(
            Some(path.to_path_buf()),
            "unknown Terlan artifact-template suffix",
        )]);
    };

    match target {
        ArtifactTemplateTarget::Html | ArtifactTemplateTarget::Markdown => {
            parse_template(source, path).map(|_| ())
        }
        ArtifactTemplateTarget::Json => validate_json_template_structure(source, path),
        ArtifactTemplateTarget::Toml => validate_toml_template_structure(source, path),
        ArtifactTemplateTarget::Yaml => validate_yaml_template_structure(source, path),
        ArtifactTemplateTarget::Text => validate_text_template_structure(source, path),
    }
}

/// Validates a `.terl.toml` template's static TOML structure.
///
/// Inputs:
/// - `source`: template source containing TOML plus optional `${...}`
///   interpolation islands.
/// - `path`: source path used for diagnostics.
///
/// Output:
/// - `Ok(())` when interpolation-masked source parses as TOML.
/// - `Err(Vec<HtmlDiagnostic>)` when interpolation is malformed or the masked
///   source is invalid TOML.
///
/// Transformation:
/// - Replaces interpolation islands with TOML-compatible placeholder values and
///   delegates structure validation to `basic-toml`.
pub fn validate_toml_template_structure(
    source: impl AsRef<str>,
    path: impl AsRef<Path>,
) -> Result<(), Vec<HtmlDiagnostic>> {
    let path = path.as_ref();
    let masked = mask_toml_interpolations(source.as_ref(), path)?;
    basic_toml::from_str::<Value>(&masked)
        .map(|_| ())
        .map_err(|error| {
            vec![HtmlDiagnostic::new(
                Some(path.to_path_buf()),
                format!("invalid TOML template structure: {error}"),
            )]
        })
}

/// Validates a `.terl.yaml` or `.terl.yml` template's static YAML structure.
///
/// Inputs:
/// - `source`: template source containing YAML plus optional `${...}`
///   interpolation islands.
/// - `path`: source path used for diagnostics.
///
/// Output:
/// - `Ok(())` when interpolation-masked source parses as YAML.
/// - `Err(Vec<HtmlDiagnostic>)` when interpolation is malformed or the masked
///   source is invalid YAML.
///
/// Transformation:
/// - Replaces interpolation islands with YAML-compatible placeholder values and
///   delegates structure validation to `yaml-rust`.
pub fn validate_yaml_template_structure(
    source: impl AsRef<str>,
    path: impl AsRef<Path>,
) -> Result<(), Vec<HtmlDiagnostic>> {
    let path = path.as_ref();
    let masked = mask_plain_interpolations(
        source.as_ref(),
        path,
        "YAML template interpolation",
        "__terlan_interpolation__",
    )?;
    YamlLoader::load_from_str(&masked)
        .map(|_| ())
        .map_err(|error| {
            vec![HtmlDiagnostic::new(
                Some(path.to_path_buf()),
                format!("invalid YAML template structure: {error}"),
            )]
        })
}

/// Validates a `.terl.txt` template's interpolation island boundaries.
///
/// Inputs:
/// - `source`: text template source containing optional `${...}` interpolation
///   islands.
/// - `path`: source path used for diagnostics.
///
/// Output:
/// - `Ok(())` when all interpolation islands are non-empty and terminated.
/// - `Err(Vec<HtmlDiagnostic>)` for malformed interpolation islands.
///
/// Transformation:
/// - Leaves text content otherwise unparsed, because plain-text templates have
///   no target syntax beyond Terlan expression islands.
pub fn validate_text_template_structure(
    source: impl AsRef<str>,
    path: impl AsRef<Path>,
) -> Result<(), Vec<HtmlDiagnostic>> {
    validate_interpolation_islands(
        source.as_ref(),
        path.as_ref(),
        "text template interpolation",
    )
}

/// Masks Terlan interpolation islands in non-JSON structured source.
///
/// Inputs:
/// - `source`: target source text.
/// - `path`: source path used for diagnostics.
/// - `label`: target-specific diagnostic label.
/// - `placeholder`: target-compatible text used in place of each island.
///
/// Output:
/// - Source text with interpolation islands replaced by `placeholder`.
/// - Diagnostics for empty or unterminated interpolation islands.
///
/// Transformation:
/// - Scans for `${...}` islands without parsing the target language and replaces
///   each complete, non-empty island with a caller-provided placeholder.
fn mask_plain_interpolations(
    source: &str,
    path: &Path,
    label: &str,
    placeholder: &str,
) -> Result<String, Vec<HtmlDiagnostic>> {
    let bytes = source.as_bytes();
    let mut output = String::with_capacity(source.len());
    let mut index = 0usize;

    while index < bytes.len() {
        if !starts_interpolation(bytes, index) {
            push_source_char(source, &mut output, &mut index);
            continue;
        }
        let Some(end) = find_interpolation_end(bytes, index + 2) else {
            return Err(vec![HtmlDiagnostic::new(
                Some(path.to_path_buf()),
                format!("unterminated {label}"),
            )]);
        };
        if source[index + 2..end].trim().is_empty() {
            return Err(vec![HtmlDiagnostic::new(
                Some(path.to_path_buf()),
                format!("empty {label}"),
            )]);
        }
        output.push_str(placeholder);
        index = end + 1;
    }

    Ok(output)
}

/// Validates a `.terl.json` template's static JSON structure.
///
/// Inputs:
/// - `source`: template source containing JSON plus optional `${...}`
///   interpolation islands.
/// - `path`: source path used for diagnostics.
///
/// Output:
/// - `Ok(())` when interpolation-masked source parses as JSON.
/// - `Err(Vec<HtmlDiagnostic>)` when interpolation is malformed or the masked
///   source is invalid JSON.
///
/// Transformation:
/// - Replaces interpolation islands with JSON-compatible placeholder values and
///   delegates structure validation to `serde_json`.
pub fn validate_json_template_structure(
    source: impl AsRef<str>,
    path: impl AsRef<Path>,
) -> Result<(), Vec<HtmlDiagnostic>> {
    let path = path.as_ref();
    let masked = mask_json_interpolations(source.as_ref(), path)?;
    serde_json::from_str::<Value>(&masked)
        .map(|_| ())
        .map_err(|error| {
            vec![HtmlDiagnostic::new(
                Some(path.to_path_buf()),
                format!("invalid JSON template structure: {error}"),
            )]
        })
}

/// Validates non-empty, terminated interpolation islands in free-form source.
///
/// Inputs:
/// - `source`: target source text.
/// - `path`: source path used for diagnostics.
/// - `label`: target-specific diagnostic label.
///
/// Output:
/// - `Ok(())` when every `${...}` island is non-empty and terminated.
/// - `Err(Vec<HtmlDiagnostic>)` for the first malformed island.
///
/// Transformation:
/// - Scans only for Terlan interpolation delimiters and does not validate the
///   target language or Terlan expression syntax.
fn validate_interpolation_islands(
    source: &str,
    path: &Path,
    label: &str,
) -> Result<(), Vec<HtmlDiagnostic>> {
    let bytes = source.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if !starts_interpolation(bytes, index) {
            index += 1;
            continue;
        }
        let Some(end) = find_interpolation_end(bytes, index + 2) else {
            return Err(vec![HtmlDiagnostic::new(
                Some(path.to_path_buf()),
                format!("unterminated {label}"),
            )]);
        };
        if source[index + 2..end].trim().is_empty() {
            return Err(vec![HtmlDiagnostic::new(
                Some(path.to_path_buf()),
                format!("empty {label}"),
            )]);
        }
        index = end + 1;
    }
    Ok(())
}

/// Masks Terlan interpolation islands inside JSON source.
///
/// Inputs:
/// - `source`: raw `.terl.json` template source.
/// - `path`: source path used for diagnostics.
///
/// Output:
/// - JSON text where interpolations have been replaced by valid JSON
///   placeholders.
/// - Diagnostics for empty or unterminated interpolation islands.
///
/// Transformation:
/// - Tracks JSON string state so `${...}` inside strings becomes static text
///   while `${...}` in value positions becomes `null`.
fn mask_json_interpolations(source: &str, path: &Path) -> Result<String, Vec<HtmlDiagnostic>> {
    let bytes = source.as_bytes();
    let mut output = String::with_capacity(source.len());
    let mut index = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            if escaped {
                escaped = false;
                push_source_char(source, &mut output, &mut index);
                continue;
            }
            if byte == b'\\' {
                escaped = true;
                output.push('\\');
                index += 1;
                continue;
            }
            if byte == b'"' {
                in_string = false;
                output.push('"');
                index += 1;
                continue;
            }
        } else if byte == b'"' {
            in_string = true;
            output.push('"');
            index += 1;
            continue;
        }

        if starts_interpolation(bytes, index) {
            let end = if in_string {
                find_string_interpolation_end(bytes, index + 2)
            } else {
                find_interpolation_end(bytes, index + 2)
            };
            let Some(end) = end else {
                return Err(vec![HtmlDiagnostic::new(
                    Some(path.to_path_buf()),
                    "unterminated JSON template interpolation",
                )]);
            };
            let expression = source[index + 2..end].trim();
            if expression.is_empty() {
                return Err(vec![HtmlDiagnostic::new(
                    Some(path.to_path_buf()),
                    "empty JSON template interpolation",
                )]);
            }
            if in_string {
                output.push_str("__terlan_interpolation__");
            } else {
                output.push_str("null");
            }
            index = end + 1;
            continue;
        }

        push_source_char(source, &mut output, &mut index);
    }

    Ok(output)
}

/// Masks Terlan interpolation islands inside TOML source.
///
/// Inputs:
/// - `source`: raw `.terl.toml` template source.
/// - `path`: source path used for diagnostics.
///
/// Output:
/// - TOML text where interpolations have been replaced by valid TOML
///   placeholders.
/// - Diagnostics for empty or unterminated interpolation islands.
///
/// Transformation:
/// - Tracks basic single- and double-quoted TOML strings so `${...}` inside
///   strings becomes static text while `${...}` in value positions becomes a
///   quoted placeholder string.
fn mask_toml_interpolations(source: &str, path: &Path) -> Result<String, Vec<HtmlDiagnostic>> {
    let bytes = source.as_bytes();
    let mut output = String::with_capacity(source.len());
    let mut index = 0usize;
    let mut string_quote: Option<u8> = None;
    let mut escaped = false;

    while index < bytes.len() {
        let byte = bytes[index];
        if starts_interpolation(bytes, index) {
            let Some(end) = find_interpolation_end(bytes, index + 2) else {
                return Err(vec![HtmlDiagnostic::new(
                    Some(path.to_path_buf()),
                    "unterminated TOML template interpolation",
                )]);
            };
            if source[index + 2..end].trim().is_empty() {
                return Err(vec![HtmlDiagnostic::new(
                    Some(path.to_path_buf()),
                    "empty TOML template interpolation",
                )]);
            }
            if string_quote.is_some() {
                output.push_str("__terlan_interpolation__");
            } else {
                output.push_str("\"__terlan_interpolation__\"");
            }
            index = end + 1;
            escaped = false;
            continue;
        }

        push_source_char(source, &mut output, &mut index);
        match string_quote {
            Some(b'"') if escaped => escaped = false,
            Some(b'"') if byte == b'\\' => escaped = true,
            Some(quote) if byte == quote => string_quote = None,
            None if matches!(byte, b'"' | b'\'') => string_quote = Some(byte),
            _ => {}
        }
    }

    Ok(output)
}

/// Copies one UTF-8 character from source into output.
///
/// Inputs:
/// - `source`: original template source.
/// - `output`: masked template output being built.
/// - `index`: byte offset of the next source character.
///
/// Output:
/// - Mutates `output` and advances `index` to the next UTF-8 boundary.
///
/// Transformation:
/// - Preserves non-ASCII text exactly instead of converting raw bytes into
///   unrelated Unicode scalar values.
fn push_source_char(source: &str, output: &mut String, index: &mut usize) {
    let Some(ch) = source[*index..].chars().next() else {
        return;
    };
    output.push(ch);
    *index += ch.len_utf8();
}

/// Returns whether an interpolation starts at `index`.
///
/// Inputs:
/// - `bytes`: source bytes.
/// - `index`: candidate byte offset.
///
/// Output:
/// - `true` when bytes at `index` begin with `${`.
///
/// Transformation:
/// - Performs bounded byte comparison without allocating.
fn starts_interpolation(bytes: &[u8], index: usize) -> bool {
    bytes.get(index) == Some(&b'$') && bytes.get(index + 1) == Some(&b'{')
}

/// Finds the end of one interpolation island.
///
/// Inputs:
/// - `bytes`: source bytes.
/// - `start`: byte offset immediately after `${`.
///
/// Output:
/// - Closing brace byte offset, or `None`.
///
/// Transformation:
/// - Scans until the first `}`. Nested Terlan expressions are intentionally
///   deferred until expression-island parsing is implemented.
fn find_interpolation_end(bytes: &[u8], start: usize) -> Option<usize> {
    bytes[start..]
        .iter()
        .position(|byte| *byte == b'}')
        .map(|offset| start + offset)
}

/// Finds the end of one interpolation island inside a JSON string.
///
/// Inputs:
/// - `bytes`: source bytes.
/// - `start`: byte offset immediately after `${`.
///
/// Output:
/// - Closing interpolation brace byte offset, or `None` when the JSON string
///   ends first.
///
/// Transformation:
/// - Scans until `}` while respecting JSON string escapes and treating an
///   unescaped `"` as the end of the string rather than part of the
///   interpolation expression.
fn find_string_interpolation_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if escaped {
            escaped = false;
            index += 1;
            continue;
        }
        if byte == b'\\' {
            escaped = true;
            index += 1;
            continue;
        }
        if byte == b'"' {
            return None;
        }
        if byte == b'}' {
            return Some(index);
        }
        index += 1;
    }
    None
}

#[cfg(test)]
#[path = "structured_test.rs"]
mod structured_test;
