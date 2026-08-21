use std::path::Path;

use crate::terlan_html::header::{
    annotation_array_value_for_key, annotation_object_value_for_key, path_uses_terlan_header,
    template_header_metadata_entry, template_header_metadata_segments_on_line,
    walk_template_header,
};
use crate::terlan_html::HtmlDiagnostic;

const PAGE_ANNOTATION_KEYS: &[&str] = &[
    "title",
    "route",
    "layout",
    "description",
    "section",
    "nav_title",
    "parent",
    "weight",
    "kind",
    "date",
    "aliases",
    "summary",
    "authors",
    "tags",
    "draft",
];
const TEMPLATE_ANNOTATION_KEYS: &[&str] = &["name", "params"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Static page metadata extracted from a Terlan template header.
///
/// Inputs: leading `@page` annotation metadata from `.terl.md` or `.terl.html`
/// files. Output: optional route, title, and layout values. Transformation:
/// keeps page-discovery metadata typed so static-site code does not reparse
/// template headers.
pub struct PageMetadata {
    pub title: Option<String>,
    pub route: Option<String>,
    pub layout: Option<String>,
    pub description: Option<String>,
    pub section: Option<String>,
    pub nav_title: Option<String>,
    pub parent: Option<String>,
    pub weight: Option<i32>,
    pub kind: Option<String>,
    pub date: Option<String>,
    pub aliases: Vec<String>,
    pub summary: Option<String>,
    pub authors: Vec<String>,
    pub tags: Vec<String>,
    pub draft: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Static template metadata extracted from a Terlan template header.
///
/// Inputs: leading `@template` annotation metadata from `.terl.html` or
/// `.terl.md` files. Output: optional public template name and ordered
/// parameter metadata. Transformation: converts annotation-backed template
/// signatures into typed data before generated template functions exist.
pub struct TemplateMetadata {
    pub name: Option<String>,
    pub params_declared: bool,
    pub params: Vec<TemplateParamMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// One parameter declared by `@template.params`.
///
/// Inputs: one nested `params` annotation entry. Output: parameter name and
/// Terlan type text. Transformation: preserves type text for later syntax/type
/// validation without inventing a template-only type parser.
pub struct TemplateParamMetadata {
    pub name: String,
    pub type_text: String,
}

/// Extracts static page metadata from a Terlan template header.
///
/// Inputs:
/// - `source`: template or Markdown source.
/// - `path`: source path used for suffix detection and diagnostics.
///
/// Output:
/// - Page metadata from a leading `@page` annotation, or empty metadata when no
///   page annotation is present.
///
/// Transformation:
/// - Walks the same leading header region as template parsing and validates
///   imports/annotations before converting top-level `@page` values into a
///   typed metadata struct.
pub fn extract_page_metadata(
    source: impl AsRef<str>,
    path: impl AsRef<Path>,
) -> Result<PageMetadata, Vec<HtmlDiagnostic>> {
    let path = path.as_ref();
    if !path_uses_terlan_header(path) {
        return Ok(PageMetadata::default());
    }

    let mut metadata = PageMetadata::default();
    walk_template_header(source.as_ref(), path, |annotation_name, lines| {
        if annotation_name == "page" {
            metadata = page_metadata_from_annotation_lines(lines, path)?;
        }
        Ok(())
    })?;

    Ok(metadata)
}

/// Extracts static template metadata from a Terlan template header.
///
/// Inputs:
/// - `source`: template or Markdown source.
/// - `path`: source path used for suffix detection and diagnostics.
///
/// Output:
/// - Template metadata from a leading `@template` annotation, or an empty
///   signature when no template annotation is present.
///
/// Transformation:
/// - Walks validated Terlan template headers and converts `@template.name` plus
///   `@template.params` into a typed signature surface for downstream
///   generated-template work.
pub fn extract_template_metadata(
    source: impl AsRef<str>,
    path: impl AsRef<Path>,
) -> Result<TemplateMetadata, Vec<HtmlDiagnostic>> {
    let path = path.as_ref();
    if !path_uses_terlan_header(path) {
        return Ok(TemplateMetadata::default());
    }

    let mut metadata = TemplateMetadata::default();
    walk_template_header(source.as_ref(), path, |annotation_name, lines| {
        if annotation_name == "template" {
            metadata = template_metadata_from_annotation_lines(lines, path)?;
        }
        Ok(())
    })?;

    Ok(metadata)
}

/// Extracts page metadata from one consumed `@page` annotation.
///
/// Inputs:
/// - `lines`: source lines consumed by the annotation.
/// - `path`: source path used for diagnostics.
///
/// Output:
/// - Parsed page metadata or diagnostics for invalid typed metadata values.
///
/// Transformation:
/// - Reuses the top-level metadata segment scanner and converts supported
///   `@page` keys into typed string and integer values.
fn page_metadata_from_annotation_lines(
    lines: &[&str],
    path: &Path,
) -> Result<PageMetadata, Vec<HtmlDiagnostic>> {
    let mut depth = 0isize;
    let mut metadata = PageMetadata::default();
    let mut seen_keys = Vec::new();

    for line in lines {
        for segment in template_header_metadata_segments_on_line(line, &mut depth) {
            let Some((key, value)) = template_header_metadata_entry(segment) else {
                continue;
            };
            validate_template_annotation_key("page", key, PAGE_ANNOTATION_KEYS, path)?;
            validate_unique_template_annotation_key("page", key, &mut seen_keys, path)?;
            match key {
                "aliases" | "authors" | "tags" => {}
                "draft" => {
                    metadata.draft = parse_page_boolean(value, key, path)?;
                }
                "weight" => {
                    metadata.weight = Some(parse_page_weight(value, path)?);
                }
                "title" | "route" | "layout" | "description" | "section" | "nav_title"
                | "parent" | "kind" | "date" | "summary" => {
                    let value = parse_template_header_string_value(value, key, "page", path)?;
                    match key {
                        "title" => metadata.title = Some(value),
                        "route" => metadata.route = Some(value),
                        "layout" => metadata.layout = Some(value),
                        "description" => metadata.description = Some(value),
                        "section" => metadata.section = Some(value),
                        "nav_title" => metadata.nav_title = Some(value),
                        "parent" => metadata.parent = Some(value),
                        "kind" => metadata.kind = Some(value),
                        "date" => metadata.date = Some(value),
                        "summary" => metadata.summary = Some(value),
                        _ => unreachable!("string page key was schema-validated"),
                    }
                }
                _ => unreachable!("page annotation key was schema-validated"),
            }
        }
    }

    let annotation_source = lines.concat();
    for key in ["aliases", "authors", "tags"] {
        if !seen_keys.iter().any(|seen| seen == key) {
            continue;
        }
        let Some(body) = annotation_array_value_for_key(&annotation_source, key) else {
            return Err(vec![HtmlDiagnostic::new(
                Some(path.to_path_buf()),
                format!("Terlan @page key `{key}` must be an array of string literals"),
            )]);
        };
        let values = parse_page_string_array(&body, key, path)?;
        match key {
            "aliases" => metadata.aliases = values,
            "authors" => metadata.authors = values,
            "tags" => metadata.tags = values,
            _ => unreachable!("array page key was schema-validated"),
        }
    }

    Ok(metadata)
}

/// Parses one string-array page metadata value.
fn parse_page_string_array(
    raw: &str,
    key: &str,
    path: &Path,
) -> Result<Vec<String>, Vec<HtmlDiagnostic>> {
    let mut values = Vec::new();
    let mut start = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (index, ch) in raw.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            ',' => {
                parse_page_string_array_segment(&raw[start..index], key, path, &mut values)?;
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    if in_string {
        return Err(vec![HtmlDiagnostic::new(
            Some(path.to_path_buf()),
            format!("Terlan @page key `{key}` has an unterminated string literal"),
        )]);
    }
    parse_page_string_array_segment(&raw[start..], key, path, &mut values)?;
    Ok(values)
}

/// Parses and appends one comma-delimited page string.
fn parse_page_string_array_segment(
    raw: &str,
    key: &str,
    path: &Path,
    values: &mut Vec<String>,
) -> Result<(), Vec<HtmlDiagnostic>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(());
    }
    let value = parse_template_header_string_value(raw, key, "page", path)?;
    if value.trim().is_empty() {
        return Err(vec![HtmlDiagnostic::new(
            Some(path.to_path_buf()),
            format!("Terlan @page key `{key}` must not contain empty values"),
        )]);
    }
    if values.contains(&value) {
        return Err(vec![HtmlDiagnostic::new(
            Some(path.to_path_buf()),
            format!("Terlan @page key `{key}` contains duplicate value `{value}`"),
        )]);
    }
    values.push(value);
    Ok(())
}

/// Parses a literal boolean page metadata value.
fn parse_page_boolean(raw: &str, key: &str, path: &Path) -> Result<bool, Vec<HtmlDiagnostic>> {
    match raw.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(vec![HtmlDiagnostic::new(
            Some(path.to_path_buf()),
            format!("Terlan @page key `{key}` must be a boolean literal"),
        )]),
    }
}

/// Parses the signed integer ordering value used by documentation navigation.
fn parse_page_weight(raw: &str, path: &Path) -> Result<i32, Vec<HtmlDiagnostic>> {
    raw.trim().parse::<i32>().map_err(|_| {
        vec![HtmlDiagnostic::new(
            Some(path.to_path_buf()),
            "Terlan @page key `weight` must be a signed 32-bit integer",
        )]
    })
}

/// Extracts template metadata from one consumed `@template` annotation.
///
/// Inputs:
/// - `lines`: source lines consumed by the annotation.
/// - `path`: source path used for diagnostics.
///
/// Output:
/// - Parsed template metadata or diagnostics for malformed names/params.
///
/// Transformation:
/// - Reads the shallow top-level `name` and `params` keys, preserving parameter
///   type text for the generated-template signature path.
fn template_metadata_from_annotation_lines(
    lines: &[&str],
    path: &Path,
) -> Result<TemplateMetadata, Vec<HtmlDiagnostic>> {
    let mut depth = 0isize;
    let mut metadata = TemplateMetadata::default();
    let mut saw_params = false;
    let mut seen_keys = Vec::new();

    for line in lines {
        for segment in template_header_metadata_segments_on_line(line, &mut depth) {
            let Some((key, value)) = template_header_metadata_entry(segment) else {
                continue;
            };
            validate_template_annotation_key("template", key, TEMPLATE_ANNOTATION_KEYS, path)?;
            validate_unique_template_annotation_key("template", key, &mut seen_keys, path)?;
            match key {
                "name" => {
                    metadata.name = Some(parse_template_header_string_value(
                        value, key, "template", path,
                    )?);
                }
                "params" => {
                    saw_params = true;
                }
                _ => unreachable!("template annotation key was schema-validated"),
            }
        }
    }

    if saw_params {
        let annotation_source = lines.concat();
        let Some(params_body) = annotation_object_value_for_key(&annotation_source, "params")
        else {
            return Err(vec![HtmlDiagnostic::new(
                Some(path.to_path_buf()),
                "Terlan @template key `params` must be an object",
            )]);
        };
        metadata.params = parse_template_param_metadata(&params_body, path)?;
        metadata.params_declared = true;
    } else {
        return Err(vec![HtmlDiagnostic::new(
            Some(path.to_path_buf()),
            "Terlan @template annotation requires `params`",
        )]);
    }

    Ok(metadata)
}

/// Validates a built-in template-header annotation key.
///
/// Inputs:
/// - `annotation`: annotation name without the leading `@`.
/// - `key`: key found in the annotation metadata object.
/// - `allowed`: schema-owned key names accepted by the annotation.
/// - `path`: source path used for diagnostics.
///
/// Output:
/// - `Ok(())` when `key` is allowed.
/// - A path-aware diagnostic when `key` is not part of the built-in schema.
///
/// Transformation:
/// - Applies compile-time schema validation to built-in template annotations
///   before metadata is consumed by static-site or generated-template code.
fn validate_template_annotation_key(
    annotation: &str,
    key: &str,
    allowed: &[&str],
    path: &Path,
) -> Result<(), Vec<HtmlDiagnostic>> {
    if allowed.contains(&key) {
        return Ok(());
    }

    Err(vec![HtmlDiagnostic::new(
        Some(path.to_path_buf()),
        format!("Terlan @{annotation} key `{key}` is not supported"),
    )])
}

/// Validates that a built-in annotation key is not repeated.
///
/// Inputs:
/// - `annotation`: annotation name without the leading `@`.
/// - `key`: key found in the annotation metadata object.
/// - `seen`: previously accepted key names for the current annotation block.
/// - `path`: source path used for diagnostics.
///
/// Output:
/// - `Ok(())` when `key` has not appeared before.
/// - A path-aware diagnostic when the same key is repeated.
///
/// Transformation:
/// - Records non-repeatable built-in metadata keys as they are consumed so
///   later values cannot silently overwrite earlier values.
fn validate_unique_template_annotation_key(
    annotation: &str,
    key: &str,
    seen: &mut Vec<String>,
    path: &Path,
) -> Result<(), Vec<HtmlDiagnostic>> {
    if seen.iter().any(|seen_key| seen_key == key) {
        return Err(vec![HtmlDiagnostic::new(
            Some(path.to_path_buf()),
            format!("duplicate Terlan @{annotation} key `{key}`"),
        )]);
    }

    seen.push(key.to_string());
    Ok(())
}

/// Parses nested `@template.params` entries.
///
/// Inputs:
/// - `body`: source text inside the `params = { ... }` object.
/// - `path`: source path used for diagnostics.
///
/// Output:
/// - Ordered parameter metadata.
///
/// Transformation:
/// - Splits top-level entries by newline or comma while respecting generic
///   brackets, then parses `name: Type` pairs.
fn parse_template_param_metadata(
    body: &str,
    path: &Path,
) -> Result<Vec<TemplateParamMetadata>, Vec<HtmlDiagnostic>> {
    let mut params = Vec::new();
    let mut seen = Vec::new();
    for segment in split_template_param_segments(body) {
        let Some((name, type_text)) = template_header_metadata_entry(segment.trim()) else {
            continue;
        };
        if !is_template_param_name(name) {
            return Err(vec![HtmlDiagnostic::new(
                Some(path.to_path_buf()),
                format!("Terlan @template param `{name}` must be a lower-case identifier"),
            )]);
        }
        if name == "children" {
            return Err(vec![HtmlDiagnostic::new(
                Some(path.to_path_buf()),
                "Terlan @template param `children` is reserved",
            )]);
        }
        if type_text.is_empty() {
            return Err(vec![HtmlDiagnostic::new(
                Some(path.to_path_buf()),
                format!("Terlan @template param `{name}` is missing a type"),
            )]);
        }
        if seen.contains(&name) {
            return Err(vec![HtmlDiagnostic::new(
                Some(path.to_path_buf()),
                format!("duplicate Terlan @template param `{name}`"),
            )]);
        }
        seen.push(name);
        params.push(TemplateParamMetadata {
            name: name.to_string(),
            type_text: type_text.trim().to_string(),
        });
    }
    Ok(params)
}

/// Splits nested `params` entries.
///
/// Inputs:
/// - `body`: text inside `params = { ... }`.
///
/// Output:
/// - Top-level parameter entry segments.
///
/// Transformation:
/// - Splits on commas and newlines outside strings and generic brackets.
fn split_template_param_segments(body: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    let mut start = 0usize;
    let mut bracket_depth = 0isize;
    let mut in_string = false;
    let mut escaped = false;

    for (index, ch) in body.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            ',' | '\n' if bracket_depth == 0 => {
                let segment = body[start..index].trim();
                if !segment.is_empty() {
                    segments.push(segment);
                }
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }

    let segment = body[start..].trim();
    if !segment.is_empty() {
        segments.push(segment);
    }
    segments
}

/// Returns whether a template parameter name is canonical.
///
/// Inputs:
/// - `name`: source parameter name.
///
/// Output:
/// - `true` for lower-case Terlan binding identifiers.
///
/// Transformation:
/// - Applies the same visible convention expected by generated template
///   functions without depending on the full Terlan lexer.
fn is_template_param_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// Parses one quoted annotation string value.
///
/// Inputs:
/// - `raw`: raw value source after `=` or `:`.
/// - `key`: metadata key used in diagnostics.
/// - `annotation`: annotation name used in diagnostics.
/// - `path`: source path used for diagnostics.
///
/// Output:
/// - Unescaped string value.
///
/// Transformation:
/// - Accepts simple double-quoted values and supports the small escape surface
///   needed for metadata text without evaluating arbitrary Terlan expressions.
fn parse_template_header_string_value(
    raw: &str,
    key: &str,
    annotation: &str,
    path: &Path,
) -> Result<String, Vec<HtmlDiagnostic>> {
    let Some(inner) = raw
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    else {
        return Err(vec![HtmlDiagnostic::new(
            Some(path.to_path_buf()),
            format!("Terlan @{annotation} key `{key}` must be a string literal"),
        )]);
    };

    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let Some(escaped) = chars.next() else {
            return Err(vec![HtmlDiagnostic::new(
                Some(path.to_path_buf()),
                format!("Terlan @{annotation} key `{key}` has an unterminated string escape"),
            )]);
        };
        match escaped {
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            'n' => out.push('\n'),
            't' => out.push('\t'),
            other => {
                return Err(vec![HtmlDiagnostic::new(
                    Some(path.to_path_buf()),
                    format!(
                        "Terlan @{annotation} key `{key}` has unsupported string escape `\\{other}`"
                    ),
                )]);
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
#[path = "metadata_test.rs"]
#[cfg(test)]
mod metadata_test;
