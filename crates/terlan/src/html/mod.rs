use std::path::{Path, PathBuf};

mod artifact;
mod base_path;
mod escaping;
mod header;
mod metadata;
mod parser;
mod structured;

pub use artifact::{
    artifact_template_target_from_filename, artifact_template_target_from_path,
    is_terlan_artifact_template_path, ArtifactTemplateTarget, TERLAN_HTML_TEMPLATE_SUFFIX,
    TERLAN_JSON_TEMPLATE_SUFFIX, TERLAN_MARKDOWN_TEMPLATE_SUFFIX, TERLAN_TEMPLATE_SUFFIX,
    TERLAN_TEXT_TEMPLATE_SUFFIX, TERLAN_TOML_TEMPLATE_SUFFIX, TERLAN_YAML_TEMPLATE_SUFFIX,
    TERLAN_YML_TEMPLATE_SUFFIX,
};
pub use base_path::inject_html_base_path;
pub use escaping::{escape_html_attr, escape_html_text};
pub use metadata::{
    extract_page_metadata, extract_template_metadata, PageMetadata, TemplateMetadata,
    TemplateParamMetadata,
};
pub use parser::{
    parse_html_template, parse_markdown, parse_markdown_template, parse_template, validate_css,
    validate_html_output,
};
pub use structured::{
    validate_artifact_template_structure, validate_json_template_structure,
    validate_text_template_structure, validate_toml_template_structure,
    validate_yaml_template_structure,
};

#[derive(Debug, Clone, PartialEq, Eq)]
/// Parsed Terlan template with source metadata.
///
/// Inputs: template source text and path. Output: a reusable template tree.
/// Transformation: stores the derived tag name and parsed nodes without
/// backend-specific rendering.
pub struct HtmlTemplate {
    pub source_path: Option<PathBuf>,
    pub tag_name: Option<String>,
    pub nodes: Vec<HtmlNode>,
}

impl HtmlTemplate {
    /// Creates an unnamed template from parsed nodes.
    ///
    /// Inputs: `nodes` is the parsed template body. Output: `HtmlTemplate`
    /// without source path or tag. Transformation: wraps nodes unchanged.
    pub fn new(nodes: Vec<HtmlNode>) -> Self {
        Self {
            source_path: None,
            tag_name: None,
            nodes,
        }
    }

    /// Creates a named template from a `.terl.html` or `.terl.md` path.
    ///
    /// Inputs: source `path` and parsed `nodes`. Output: template with derived
    /// tag metadata or a diagnostic. Transformation: validates the filename and
    /// converts it to the canonical custom-element tag form.
    pub fn from_terlan_template_path(
        path: impl AsRef<Path>,
        nodes: Vec<HtmlNode>,
    ) -> Result<Self, HtmlDiagnostic> {
        let path = path.as_ref();
        Ok(Self {
            source_path: Some(path.to_path_buf()),
            tag_name: Some(template_tag_from_path(path)?),
            nodes,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Template node produced by the HTML and Markdown parsers.
///
/// Inputs: HTML tokenizer events or Markdown-rendered HTML. Output: typed node
/// variants. Transformation: preserves text, structural elements, comments,
/// doctypes, and Terlan interpolation slots.
pub enum HtmlNode {
    Text(String),
    Element(HtmlElement),
    Comment(String),
    Doctype(String),
    Slot(HtmlSlot),
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Parsed HTML element node.
///
/// Inputs: start/end tag tokens and nested children. Output: element name,
/// attributes, and child nodes. Transformation: accumulates child nodes until
/// the matching close tag is observed.
pub struct HtmlElement {
    pub name: String,
    pub attrs: Vec<HtmlAttr>,
    pub children: Vec<HtmlNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Parsed HTML attribute.
///
/// Inputs: tokenizer attribute name/value. Output: typed attribute with an
/// optional value. Transformation: converts values into static text or slot
/// interpolation.
pub struct HtmlAttr {
    pub name: String,
    pub value: Option<HtmlAttrValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Parsed HTML attribute value.
///
/// Inputs: raw attribute value text. Output: static text or interpolation slot.
/// Transformation: recognizes whole-value `${slot.path}` interpolation and
/// accepts legacy `{slot.path}` interpolation while sources migrate.
pub enum HtmlAttrValue {
    Text(String),
    Slot(HtmlSlot),
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Terlan template interpolation slot.
///
/// Inputs: `${expr}` source. Output: original expression text, dotted path
/// segments when the expression is a simple path, and optional source span.
/// Transformation: preserves the source expression for compiler validation
/// while keeping dotted path metadata for static renderers.
pub struct HtmlSlot {
    pub expression: String,
    pub path: Vec<String>,
    pub span: Option<HtmlSpan>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Source span for a template interpolation slot.
///
/// Inputs: HTML tokenizer line and byte offsets. Output: line/start/end span.
/// Transformation: carries parser offsets for diagnostics and downstream
/// mapping.
pub struct HtmlSpan {
    pub line: u64,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Parsed Markdown document and rendered HTML representation.
///
/// Inputs: Markdown source and path. Output: raw Markdown, rendered HTML, and
/// parsed HTML nodes. Transformation: renders Markdown through `comrak` and
/// parses the resulting HTML with the same template parser.
pub struct MarkdownDocument {
    pub source_path: Option<PathBuf>,
    pub raw_source: String,
    pub rendered_html: String,
    pub nodes: Vec<HtmlNode>,
}

impl HtmlSlot {
    /// Builds a slot from dotted path text.
    ///
    /// Inputs: dotted path string. Output: slot without a span. Transformation:
    /// splits non-empty dot segments into a path vector.
    pub fn dotted(path: impl AsRef<str>) -> Self {
        let path_text = path.as_ref();
        Self {
            expression: path_text.to_owned(),
            path: path_text
                .split('.')
                .filter(|part| !part.is_empty())
                .map(str::to_owned)
                .collect(),
            span: None,
        }
    }

    /// Attaches a source span to a slot.
    ///
    /// Inputs: existing slot and `span`. Output: slot with span metadata.
    /// Transformation: mutates only the optional span field.
    pub fn with_span(mut self, span: HtmlSpan) -> Self {
        self.span = Some(span);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// HTML/template diagnostic.
///
/// Inputs: optional source path and message. Output: diagnostic consumed by
/// callers and CLI display. Transformation: stores path/message without
/// formatting side effects.
pub struct HtmlDiagnostic {
    pub path: Option<PathBuf>,
    pub message: String,
}

impl HtmlDiagnostic {
    /// Creates a diagnostic.
    ///
    /// Inputs: optional path and display message. Output: `HtmlDiagnostic`.
    /// Transformation: converts the message into owned text.
    pub fn new(path: Option<PathBuf>, message: impl Into<String>) -> Self {
        Self {
            path,
            message: message.into(),
        }
    }
}

/// Returns whether a path uses a Terlan template suffix.
///
/// Inputs: filesystem path. Output: `true` for `.terl.html` or `.terl.md`
/// filenames. Transformation: inspects only the filename suffix.
pub fn is_terlan_template_path(path: impl AsRef<Path>) -> bool {
    path.as_ref()
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| template_suffix(name).is_some())
}

/// Derives the template tag name from a Terlan template path.
///
/// Inputs: template path. Output: normalized custom-element tag or diagnostic.
/// Transformation: validates UTF-8 filename, checks suffix, strips suffix, and
/// normalizes the stem to kebab-case.
pub fn template_tag_from_path(path: impl AsRef<Path>) -> Result<String, HtmlDiagnostic> {
    let path = path.as_ref();
    let file_name = path
        .file_name()
        .ok_or_else(|| HtmlDiagnostic::new(Some(path.to_path_buf()), "missing template filename"))?
        .to_str()
        .ok_or_else(|| {
            HtmlDiagnostic::new(
                Some(path.to_path_buf()),
                "template filename must be valid UTF-8",
            )
        })?;

    let suffix = template_suffix(file_name).ok_or_else(|| {
        HtmlDiagnostic::new(
            Some(path.to_path_buf()),
            format!(
                "template filename must end with `{TERLAN_HTML_TEMPLATE_SUFFIX}` or `{TERLAN_MARKDOWN_TEMPLATE_SUFFIX}`"
            ),
        )
    })?;
    let stem = file_name.strip_suffix(suffix).expect("known suffix");

    normalize_template_tag(path, stem)
}

/// Returns the supported Terlan template suffix for a filename.
///
/// Inputs: `file_name` without directory context. Output: matching suffix or
/// `None`. Transformation: compares against known HTML and Markdown template
/// suffix constants.
fn template_suffix(file_name: &str) -> Option<&'static str> {
    if file_name.ends_with(TERLAN_HTML_TEMPLATE_SUFFIX) {
        Some(TERLAN_HTML_TEMPLATE_SUFFIX)
    } else if file_name.ends_with(TERLAN_MARKDOWN_TEMPLATE_SUFFIX) {
        Some(TERLAN_MARKDOWN_TEMPLATE_SUFFIX)
    } else {
        None
    }
}

/// Normalizes a template filename stem to a tag name.
///
/// Inputs: source path and suffix-stripped stem. Output: kebab-case tag or
/// diagnostic. Transformation: lowercases uppercase letters, converts `_` to
/// `-`, preserves `-`, and rejects invalid/repeated separators.
fn normalize_template_tag(path: &Path, stem: &str) -> Result<String, HtmlDiagnostic> {
    if stem.is_empty() {
        return Err(HtmlDiagnostic::new(
            Some(path.to_path_buf()),
            "template filename stem cannot be empty",
        ));
    }

    let mut tag = String::with_capacity(stem.len());
    let mut previous_was_dash = false;

    for ch in stem.chars() {
        match ch {
            'a'..='z' | '0'..='9' => {
                tag.push(ch);
                previous_was_dash = false;
            }
            'A'..='Z' => {
                tag.push(ch.to_ascii_lowercase());
                previous_was_dash = false;
            }
            '_' | '-' => {
                if tag.is_empty() || previous_was_dash {
                    return Err(HtmlDiagnostic::new(
                        Some(path.to_path_buf()),
                        "template tag name cannot start with or contain repeated separators",
                    ));
                }
                tag.push('-');
                previous_was_dash = true;
            }
            _ => {
                return Err(HtmlDiagnostic::new(
                    Some(path.to_path_buf()),
                    format!("invalid template filename character `{ch}`"),
                ));
            }
        }
    }

    if tag.ends_with('-') {
        return Err(HtmlDiagnostic::new(
            Some(path.to_path_buf()),
            "template tag name cannot end with a separator",
        ));
    }

    Ok(tag)
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod lib_test;
