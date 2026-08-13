use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;
use terlan_runtime_abi::{BoundaryError, ErrorDomain};

use super::{
    artifact_template_target_from_path, scan_template_interpolations,
    validate_artifact_template_structure, ArtifactTemplateTarget, TemplateInterpolationContext,
    TemplateInterpolationRegion,
};

/// Stable slot telemetry emitted beside structured static templates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StructuredTemplateTelemetry {
    pub target: String,
    pub slots: Vec<StructuredTemplateSlotTelemetry>,
}

/// Source and type-context metadata for one structured interpolation island.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StructuredTemplateSlotTelemetry {
    pub expression: String,
    pub context: String,
    pub expected: String,
    pub line: usize,
    pub column: usize,
}

/// Rendered structured output plus the exact telemetry used by all backends.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StructuredTemplateRender {
    pub output: String,
    pub telemetry: StructuredTemplateTelemetry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum StructuredSlotContext {
    JsonString,
    JsonValue,
    XmlAttribute,
    XmlText,
    YamlString,
    YamlValue,
    TomlString,
    TomlValue,
    Text,
}

impl StructuredSlotContext {
    fn name(self) -> &'static str {
        match self {
            Self::JsonString => "json-string",
            Self::JsonValue => "json-value",
            Self::XmlAttribute => "xml-attribute",
            Self::XmlText => "xml-text",
            Self::YamlString => "yaml-string",
            Self::YamlValue => "yaml-value",
            Self::TomlString => "toml-string",
            Self::TomlValue => "toml-value",
            Self::Text => "text",
        }
    }

    fn expected(self) -> &'static str {
        match self {
            Self::JsonValue | Self::YamlValue | Self::TomlValue => "structured-value",
            Self::JsonString
            | Self::XmlAttribute
            | Self::XmlText
            | Self::YamlString
            | Self::TomlString
            | Self::Text => "scalar-text",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum StructuredSegment {
    Text {
        value: String,
    },
    Slot {
        expression: String,
        context: StructuredSlotContext,
        label: String,
    },
}

/// Returns target-aware slot telemetry after validating template structure.
pub fn structured_template_telemetry(
    source: &str,
    path: &Path,
) -> Result<StructuredTemplateTelemetry, BoundaryError> {
    structured_template_telemetry_untyped(source, path).map_err(template_error)
}

fn structured_template_telemetry_untyped(
    source: &str,
    path: &Path,
) -> Result<StructuredTemplateTelemetry, String> {
    structured_template_telemetry_inner(source, path, None)
}

/// Returns target-aware telemetry while asserting the caller's expected target.
/// This keeps cached/code-generated metadata from silently drifting from a
/// template's explicit `.terl.*` suffix.
pub fn structured_template_telemetry_for_target(
    source: &str,
    path: &Path,
    expected: ArtifactTemplateTarget,
) -> Result<StructuredTemplateTelemetry, BoundaryError> {
    structured_template_telemetry_for_target_untyped(source, path, expected).map_err(template_error)
}

fn structured_template_telemetry_for_target_untyped(
    source: &str,
    path: &Path,
    expected: ArtifactTemplateTarget,
) -> Result<StructuredTemplateTelemetry, String> {
    structured_template_telemetry_inner(source, path, Some(expected))
}

fn structured_template_telemetry_inner(
    source: &str,
    path: &Path,
    expected: Option<ArtifactTemplateTarget>,
) -> Result<StructuredTemplateTelemetry, String> {
    let (target, segments) = structured_segments(source, path, expected)?;
    let slots = segments
        .into_iter()
        .filter_map(|segment| match segment {
            StructuredSegment::Slot {
                expression,
                context,
                label,
            } => {
                let (path_and_line, column) = label.rsplit_once(':')?;
                let (_, line) = path_and_line.rsplit_once(':')?;
                Some(StructuredTemplateSlotTelemetry {
                    expression,
                    context: context.name().to_string(),
                    expected: context.expected().to_string(),
                    line: line.parse().ok()?,
                    column: column.parse().ok()?,
                })
            }
            StructuredSegment::Text { .. } => None,
        })
        .collect();
    Ok(StructuredTemplateTelemetry {
        target: target.name().to_string(),
        slots,
    })
}

/// Renders JSON/XML/YAML/TOML/text templates with target-specific escaping.
pub fn render_structured_template(
    source: &str,
    path: &Path,
    values: &BTreeMap<String, Value>,
) -> Result<StructuredTemplateRender, BoundaryError> {
    render_structured_template_untyped(source, path, values).map_err(template_error)
}

fn render_structured_template_untyped(
    source: &str,
    path: &Path,
    values: &BTreeMap<String, Value>,
) -> Result<StructuredTemplateRender, String> {
    let (_target, segments) = structured_segments(source, path, None)?;
    let telemetry = structured_template_telemetry_untyped(source, path)?;
    let mut output = String::new();
    for segment in segments {
        match segment {
            StructuredSegment::Text { value } => output.push_str(&value),
            StructuredSegment::Slot {
                expression,
                context,
                label,
            } => {
                let value = values.get(&expression).ok_or_else(|| {
                    format!("error[template_backend_missing_slot]: {label}: missing slot `{expression}`")
                })?;
                output.push_str(&render_value(value, context, &expression, &label)?);
            }
        }
    }
    validate_artifact_template_structure(&output, path).map_err(|diagnostics| {
        format!(
            "error[template_backend_rendered_structure]: {}: {}",
            path.display(),
            diagnostics
                .into_iter()
                .map(|diagnostic| diagnostic.message)
                .collect::<Vec<_>>()
                .join("; ")
        )
    })?;
    Ok(StructuredTemplateRender { output, telemetry })
}

/// Emits a self-contained browser renderer from the same structured descriptor
/// used by the Rust/static path.
pub fn emit_structured_template_browser_renderer(
    source: &str,
    path: &Path,
    export_name: &str,
) -> Result<String, BoundaryError> {
    emit_structured_template_browser_renderer_untyped(source, path, export_name)
        .map_err(template_error)
}

fn emit_structured_template_browser_renderer_untyped(
    source: &str,
    path: &Path,
    export_name: &str,
) -> Result<String, String> {
    if export_name.is_empty()
        || !export_name.chars().enumerate().all(|(index, ch)| {
            ch == '_' || ch.is_ascii_alphanumeric() && (index > 0 || !ch.is_ascii_digit())
        })
    {
        return Err(format!(
            "error[template_backend_js_identifier]: invalid renderer export `{export_name}`"
        ));
    }
    let (_, segments) = structured_segments(source, path, None)?;
    let telemetry = structured_template_telemetry_untyped(source, path)?;
    let descriptor = serde_json::to_string(&segments)
        .map_err(|error| format!("failed to encode structured template descriptor: {error}"))?;
    let telemetry = serde_json::to_string(&telemetry)
        .map_err(|error| format!("failed to encode structured template telemetry: {error}"))?;
    let prefix = format!("__terlanStructured_{export_name}");
    Ok(format!(
        r#"const {prefix}Segments = {descriptor};
const {prefix}Telemetry = {telemetry};
function {prefix}Scalar(value, expression, label) {{
  if (typeof value !== "string" && typeof value !== "number" && typeof value !== "boolean") {{
    throw new TypeError(`error[template_backend_slot_type]: ${{label}}: slot \`${{expression}}\` requires scalar-text`);
  }}
  return String(value);
}}
function {prefix}EscapeXml(value, attribute) {{
  const pattern = attribute ? /[&"<>]/g : /[&<>]/g;
  return value.replace(pattern, (character) => ({{ "&": "&amp;", "\"": "&quot;", "<": "&lt;", ">": "&gt;" }})[character]);
}}
function {prefix}Value(value, context, expression, label) {{
  if (context === "json-value" || context === "yaml-value" || context === "toml-value") {{
    const rendered = JSON.stringify(value);
    if (rendered === undefined) throw new TypeError(`error[template_backend_slot_type]: ${{label}}: slot \`${{expression}}\` requires structured-value`);
    return rendered;
  }}
  const scalar = {prefix}Scalar(value, expression, label);
  if (context === "json-string" || context === "yaml-string" || context === "toml-string") return JSON.stringify(scalar).slice(1, -1);
  if (context === "xml-attribute") return {prefix}EscapeXml(scalar, true);
  if (context === "xml-text") return {prefix}EscapeXml(scalar, false);
  return scalar;
}}
export function {export_name}(props) {{
  let output = "";
  for (const segment of {prefix}Segments) {{
    if (segment.kind === "text") output += segment.value;
    else {{
      if (!Object.hasOwn(props, segment.expression)) throw new TypeError(`error[template_backend_missing_slot]: ${{segment.label}}: missing slot \`${{segment.expression}}\``);
      output += {prefix}Value(props[segment.expression], segment.context, segment.expression, segment.label);
    }}
  }}
  return {{ output, telemetry: {prefix}Telemetry }};
}}
"#
    ))
}

fn template_error(error: String) -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::TemplateRendering,
        "render structured template",
        error,
    )
}

fn structured_segments(
    source: &str,
    path: &Path,
    expected: Option<ArtifactTemplateTarget>,
) -> Result<(ArtifactTemplateTarget, Vec<StructuredSegment>), String> {
    let regions = scan_template_interpolations(source).map_err(|error| {
        format!(
            "error[template_target_structure]: {}:{}:{}: {}",
            path.display(),
            error.line,
            error.start + 1,
            error.message
        )
    })?;
    let first_location = regions
        .first()
        .map(|region| source_location(source, region.open_start))
        .unwrap_or((1, 1));
    let target = artifact_template_target_from_path(path).ok_or_else(|| {
        let (line, column) = first_location;
        format!(
            "error[template_target_unknown]: {}:{line}:{column}: unsupported template target suffix",
            path.display()
        )
    })?;
    if expected.is_some_and(|expected| expected != target) {
        let expected = expected.expect("checked above");
        let (line, column) = first_location;
        return Err(format!(
            "error[template_target_mismatch]: {}:{line}:{column}: expected {} template, inferred {} from suffix",
            path.display(), expected.name(), target.name()
        ));
    }
    if matches!(
        target,
        ArtifactTemplateTarget::Html | ArtifactTemplateTarget::Markdown
    ) {
        let (line, column) = first_location;
        return Err(format!(
            "error[template_target_mismatch]: {}:{line}:{column}: structured renderer cannot render {} templates",
            path.display(),
            target.name()
        ));
    }
    validate_artifact_template_structure(source, path).map_err(|diagnostics| {
        format!(
            "error[template_target_structure]: {}: {}",
            path.display(),
            diagnostics
                .into_iter()
                .map(|diagnostic| diagnostic.message)
                .collect::<Vec<_>>()
                .join("; ")
        )
    })?;

    let mut segments = Vec::new();
    let mut text_start = 0usize;
    for region in regions {
        if text_start < region.open_start {
            segments.push(StructuredSegment::Text {
                value: source[text_start..region.open_start].to_string(),
            });
        }
        let expression = source[region.expression_start..region.expression_end]
            .trim()
            .to_string();
        let context = slot_context(target, source, &region);
        let (line, column) = source_location(source, region.open_start);
        segments.push(StructuredSegment::Slot {
            expression,
            context,
            label: format!("{}:{line}:{column}", path.display()),
        });
        text_start = region.close_end;
    }
    if text_start < source.len() {
        segments.push(StructuredSegment::Text {
            value: source[text_start..].to_string(),
        });
    }
    Ok((target, segments))
}

fn slot_context(
    target: ArtifactTemplateTarget,
    source: &str,
    region: &TemplateInterpolationRegion,
) -> StructuredSlotContext {
    let quote = quote_at(source, region.open_start);
    match target {
        ArtifactTemplateTarget::Json if quote.is_some() => StructuredSlotContext::JsonString,
        ArtifactTemplateTarget::Json => StructuredSlotContext::JsonValue,
        ArtifactTemplateTarget::Xml
            if matches!(
                region.context,
                TemplateInterpolationContext::Attribute { .. }
            ) =>
        {
            StructuredSlotContext::XmlAttribute
        }
        ArtifactTemplateTarget::Xml => StructuredSlotContext::XmlText,
        ArtifactTemplateTarget::Yaml if quote.is_some() => StructuredSlotContext::YamlString,
        ArtifactTemplateTarget::Yaml => StructuredSlotContext::YamlValue,
        ArtifactTemplateTarget::Toml if quote.is_some() => StructuredSlotContext::TomlString,
        ArtifactTemplateTarget::Toml => StructuredSlotContext::TomlValue,
        ArtifactTemplateTarget::Text => StructuredSlotContext::Text,
        ArtifactTemplateTarget::Html | ArtifactTemplateTarget::Markdown => unreachable!(),
    }
}

fn quote_at(source: &str, offset: usize) -> Option<u8> {
    let mut quote = None;
    let mut escaped = false;
    for byte in source.as_bytes()[..offset].iter().copied() {
        if escaped {
            escaped = false;
        } else if quote.is_some() && byte == b'\\' {
            escaped = true;
        } else if quote == Some(byte) {
            quote = None;
        } else if quote.is_none() && matches!(byte, b'"' | b'\'') {
            quote = Some(byte);
        }
    }
    quote
}

fn render_value(
    value: &Value,
    context: StructuredSlotContext,
    expression: &str,
    label: &str,
) -> Result<String, String> {
    if matches!(
        context,
        StructuredSlotContext::JsonValue
            | StructuredSlotContext::YamlValue
            | StructuredSlotContext::TomlValue
    ) {
        return serde_json::to_string(value)
            .map_err(|_| slot_type_error(label, expression, context));
    }
    let scalar = match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        _ => return Err(slot_type_error(label, expression, context)),
    };
    match context {
        StructuredSlotContext::JsonString
        | StructuredSlotContext::YamlString
        | StructuredSlotContext::TomlString => {
            let encoded = serde_json::to_string(&scalar).expect("strings serialize as JSON");
            Ok(encoded[1..encoded.len() - 1].to_string())
        }
        StructuredSlotContext::XmlAttribute => Ok(escape_xml(&scalar, true)),
        StructuredSlotContext::XmlText => Ok(escape_xml(&scalar, false)),
        StructuredSlotContext::Text => Ok(scalar),
        StructuredSlotContext::JsonValue
        | StructuredSlotContext::YamlValue
        | StructuredSlotContext::TomlValue => unreachable!(),
    }
}

fn slot_type_error(label: &str, expression: &str, context: StructuredSlotContext) -> String {
    format!(
        "error[template_backend_slot_type]: {label}: slot `{expression}` requires {}",
        context.expected()
    )
}

fn escape_xml(value: &str, attribute: bool) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' if attribute => escaped.push_str("&quot;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn source_location(source: &str, offset: usize) -> (usize, usize) {
    let prefix = &source[..offset.min(source.len())];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix.chars().count() + 1, |(_, tail)| {
            tail.chars().count() + 1
        });
    (line, column)
}

#[cfg(test)]
#[path = "structured_render_test.rs"]
#[cfg(test)]
mod structured_render_test;
