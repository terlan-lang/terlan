use std::path::Path;

pub const TERLAN_HTML_TEMPLATE_SUFFIX: &str = ".terl.html";
pub const TERLAN_MARKDOWN_TEMPLATE_SUFFIX: &str = ".terl.md";
pub const TERLAN_JSON_TEMPLATE_SUFFIX: &str = ".terl.json";
pub const TERLAN_TOML_TEMPLATE_SUFFIX: &str = ".terl.toml";
pub const TERLAN_YAML_TEMPLATE_SUFFIX: &str = ".terl.yaml";
pub const TERLAN_YML_TEMPLATE_SUFFIX: &str = ".terl.yml";
pub const TERLAN_TEXT_TEMPLATE_SUFFIX: &str = ".terl.txt";
pub const TERLAN_TEMPLATE_SUFFIX: &str = TERLAN_HTML_TEMPLATE_SUFFIX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Target format for a Terlan artifact template file.
///
/// Inputs: derived from a template filename suffix. Output: target format
/// classification. Transformation: separates target selection from parsing so
/// HTML, Markdown, JSON, TOML, YAML, and text templates can share discovery and
/// diagnostics without sharing render implementations.
pub enum ArtifactTemplateTarget {
    Html,
    Markdown,
    Json,
    Toml,
    Yaml,
    Text,
}

impl ArtifactTemplateTarget {
    /// Returns the canonical Terlan suffix for this target.
    ///
    /// Inputs: target variant. Output: suffix string. Transformation: maps the
    /// enum into the file extension contract used by project discovery.
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Html => TERLAN_HTML_TEMPLATE_SUFFIX,
            Self::Markdown => TERLAN_MARKDOWN_TEMPLATE_SUFFIX,
            Self::Json => TERLAN_JSON_TEMPLATE_SUFFIX,
            Self::Toml => TERLAN_TOML_TEMPLATE_SUFFIX,
            Self::Yaml => TERLAN_YAML_TEMPLATE_SUFFIX,
            Self::Text => TERLAN_TEXT_TEMPLATE_SUFFIX,
        }
    }

    /// Returns the stable diagnostic name for this target.
    ///
    /// Inputs: target variant. Output: lowercase target name. Transformation:
    /// exposes a stable label for compiler diagnostics and manifests.
    pub fn name(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Markdown => "markdown",
            Self::Json => "json",
            Self::Toml => "toml",
            Self::Yaml => "yaml",
            Self::Text => "text",
        }
    }

    /// Returns whether this target is represented as an HTML template tree.
    ///
    /// Inputs: target variant. Output: `true` for HTML and Markdown.
    /// Transformation: preserves the existing parser split while allowing
    /// broader artifact-template discovery.
    pub fn parses_to_html_tree(self) -> bool {
        matches!(self, Self::Html | Self::Markdown)
    }
}

/// Returns whether a path uses any Terlan artifact-template suffix.
///
/// Inputs: filesystem path. Output: `true` for supported `.terl.*` artifact
/// template filenames. Transformation: inspects only the filename suffix and
/// does not parse the template body.
pub fn is_terlan_artifact_template_path(path: impl AsRef<Path>) -> bool {
    artifact_template_target_from_path(path).is_some()
}

/// Classifies a path by Terlan artifact-template target.
///
/// Inputs: filesystem path. Output: target classification or `None`.
/// Transformation: extracts the UTF-8 filename and delegates to suffix
/// classification without touching the filesystem.
pub fn artifact_template_target_from_path(
    path: impl AsRef<Path>,
) -> Option<ArtifactTemplateTarget> {
    path.as_ref()
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(artifact_template_target_from_filename)
}

/// Classifies a filename by Terlan artifact-template target.
///
/// Inputs: filename without required directory context. Output: target
/// classification or `None`. Transformation: compares known suffixes in a
/// deterministic order and normalizes `.terl.yml` to the YAML target.
pub fn artifact_template_target_from_filename(file_name: &str) -> Option<ArtifactTemplateTarget> {
    if file_name.ends_with(TERLAN_HTML_TEMPLATE_SUFFIX) {
        Some(ArtifactTemplateTarget::Html)
    } else if file_name.ends_with(TERLAN_MARKDOWN_TEMPLATE_SUFFIX) {
        Some(ArtifactTemplateTarget::Markdown)
    } else if file_name.ends_with(TERLAN_JSON_TEMPLATE_SUFFIX) {
        Some(ArtifactTemplateTarget::Json)
    } else if file_name.ends_with(TERLAN_TOML_TEMPLATE_SUFFIX) {
        Some(ArtifactTemplateTarget::Toml)
    } else if file_name.ends_with(TERLAN_YAML_TEMPLATE_SUFFIX)
        || file_name.ends_with(TERLAN_YML_TEMPLATE_SUFFIX)
    {
        Some(ArtifactTemplateTarget::Yaml)
    } else if file_name.ends_with(TERLAN_TEXT_TEMPLATE_SUFFIX) {
        Some(ArtifactTemplateTarget::Text)
    } else {
        None
    }
}

#[cfg(test)]
#[path = "artifact_test.rs"]
mod artifact_test;
