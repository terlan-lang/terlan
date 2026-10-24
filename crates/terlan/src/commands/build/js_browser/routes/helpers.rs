use std::path::{Component, Path};

use crate::terlan_syntax::SyntaxExprOutput;

use super::super::manifest::WebSourceSpanArtifact;
use super::super::WebRouteSourceArtifact;

/// Source metadata used while extracting route manifests.
///
/// Inputs:
/// - Produced from one route-source artifact and its source text.
///
/// Output:
/// - Package-safe source path and borrowed source text for span conversion.
///
/// Transformation:
/// - Separates absolute build paths from runtime-visible source metadata so
///   generated manifests can point diagnostics at source files without leaking
///   local machine paths.
pub(super) struct WebRouteSourceContext<'a> {
    pub(super) manifest_path: String,
    pub(super) source_text: &'a str,
}

/// Builds a route source context for manifest diagnostics.
///
/// Inputs:
/// - `source`: route source artifact produced by build discovery.
/// - `source_text`: source file contents used for line/column mapping.
///
/// Output:
/// - `WebRouteSourceContext` with package-safe manifest path and borrowed text.
///
/// Transformation:
/// - Converts the absolute source path into a stable manifest path while
///   preserving source text for span conversion.
pub(super) fn route_source_context<'a>(
    source: &WebRouteSourceArtifact,
    source_text: &'a str,
) -> WebRouteSourceContext<'a> {
    WebRouteSourceContext {
        manifest_path: source
            .manifest_path
            .clone()
            .unwrap_or_else(|| route_source_manifest_path(&source.source_path)),
        source_text,
    }
}

/// Converts a router expression span into manifest source metadata.
///
/// Inputs:
/// - `source`: source context for the file being inspected.
/// - `expr`: syntax-output expression whose span anchors the route call.
///
/// Output:
/// - Serializable source span with package-safe path and one-based position.
///
/// Transformation:
/// - Converts byte offsets into line/column pairs and clones only the safe
///   manifest-visible source path.
pub(super) fn source_span_for_expr(
    source: &WebRouteSourceContext<'_>,
    expr: &SyntaxExprOutput,
) -> WebSourceSpanArtifact {
    let (line, column) = line_column_for_offset(source.source_text, expr.span.start);
    WebSourceSpanArtifact {
        path: source.manifest_path.clone(),
        line,
        column,
    }
}

/// Normalizes a source path for route manifest output.
///
/// Inputs:
/// - `source_path`: absolute or relative source file path.
///
/// Output:
/// - Relative, slash-separated path safe for runtime manifests.
///
/// Transformation:
/// - Prefers the path from the first `src` component onward. When a temporary
///   test path or unknown absolute path has no `src`, falls back to the file
///   name so manifests never embed host-specific absolute directories.
fn route_source_manifest_path(source_path: &str) -> String {
    let path = Path::new(source_path);
    let components = path.components().collect::<Vec<_>>();
    if let Some(index) = components
        .iter()
        .position(|component| {
            matches!(component, Component::Normal(value) if value.to_string_lossy() == "src")
        })
    {
        return components_to_manifest_path(&components[index..]);
    }
    if path.is_relative() {
        let relative = components_to_manifest_path(&components);
        if !relative.is_empty() {
            return relative;
        }
    }
    path.file_name()
        .map(|name| name.to_string_lossy().replace('\\', "/"))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "unknown.terl".to_string())
}

/// Converts filesystem path components into manifest path text.
///
/// Inputs:
/// - `components`: path components selected for manifest output.
///
/// Output:
/// - Slash-separated relative path with unsafe prefix components removed.
///
/// Transformation:
/// - Keeps only normal components and drops roots, drive prefixes, current
///   directories, and parent markers.
fn components_to_manifest_path(components: &[Component<'_>]) -> String {
    components
        .iter()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Converts a byte offset into a one-based source position.
///
/// Inputs:
/// - `source`: UTF-8 source text.
/// - `offset`: byte offset from syntax-output span metadata.
///
/// Output:
/// - `(line, column)` pair, both one-based.
///
/// Transformation:
/// - Walks character boundaries up to the requested byte offset, incrementing
///   line on newline and column otherwise. Offsets beyond the source clamp to
///   the end of the source.
fn line_column_for_offset(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut column = 1usize;
    for (index, ch) in source.char_indices() {
        if index >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}
