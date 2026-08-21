use std::fs;
use std::path::{Path, PathBuf};

use crate::terlan_quality::QualityResult;

/// Summary produced by the native-boundary terminology gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeBoundaryTerminologySummary {
    pub checked_doc_count: usize,
}

const GLOSSARY_PATH: &str = "docs/runtime/NATIVE_BOUNDARY_GLOSSARY.md";

const REQUIRED_GLOSSARY_TERMS: &[&str] = &[
    "NativeBoundary",
    "NativeModule",
    "NativeResource",
    "HostCapability",
    "typed manifests",
    "capability checks",
    "resource handles",
    "lifecycle cleanup",
    "scheduler accounting",
    "async isolation",
    "typed failure propagation",
];

const PLACEHOLDER_TERMS: &[&str] = &["todo", "tbd", "placeholder", "fixme"];

const DOC_ROOTS: &[&str] = &["docs/runtime", "docs/package"];

const SELECTED_DOC_PATHS: &[&str] = &[
    "crates/terlan/src/commands/build/README.md",
    "crates/terlan/src/commands/build/js_browser.rs",
    "crates/terlan/src/commands/build/js_browser/routes.rs",
    "crates/terlan/src/commands/build/js_browser/manifest.rs",
    "crates/terlan/src/commands/init/README.md",
    "crates/terlan/src/commands/init/mod.rs",
    "crates/terlan/src/commands/serve/README.md",
    "crates/terlan/src/commands/serve/handler/types.rs",
    "crates/terlan/src/commands/serve/manifest.rs",
    "crates/terlan/src/web_route.rs",
    "std/http/README.md",
];

const FORBIDDEN_WEB_HANDLER_DOC_FRAGMENTS: &[&str] = &[
    "Use the default VM",
    "VM-backed handler",
    "VM handler bridge internal",
    "dispatches into VM-backed",
    "temporary VM bridge",
    "VM-backed handlers remain",
    "internal VM response tuple",
    "VM handler metadata",
    "VM eval request",
];

const REQUIRED_DIAGNOSTIC_MARKERS: &[(&str, &str)] = &[
    (
        "crates/terlan/src/runtime/native_boundary/error.rs",
        "NativeBoundary handle is stale or does not match the resource slot.",
    ),
    (
        "crates/terlan/src/runtime/native_boundary/error.rs",
        "NativeBoundary backpressure limit was exceeded.",
    ),
    (
        "crates/terlan/src/runtime/native_boundary/resource.rs",
        "NativeBoundary resource handle",
    ),
    (
        "crates/terlan/src/runtime/native_boundary/dispatch/args.rs",
        "No NativeBoundary adapter is registered",
    ),
];

const NIF_ALLOWED_CONTEXTS: &[&str] = &[
    "old NIF-era name",
    "NIF ABI compatibility",
    "not a NIF ABI contract",
    "not NIF calls",
];

/// Runs the native-boundary terminology gate.
///
/// Inputs:
/// - `root`: repository root containing golden docs.
///
/// Output:
/// - Success summary when the glossary exists and runtime documentation uses
///   the canonical native-boundary model.
/// - Stable diagnostics when the old name appears outside the glossary or NIF
///   terminology appears outside an explicit historical/out-of-contract
///   context.
///
/// Transformation:
/// - Validates the glossary terms and scans golden runtime/package docs for
///   new terminology drift and casual NIF framing.
pub fn run_native_boundary_terminology(
    root: &Path,
) -> QualityResult<NativeBoundaryTerminologySummary> {
    let mut diagnostics = Vec::new();
    let glossary = read_text(root, GLOSSARY_PATH)?;
    diagnostics.extend(validate_glossary_text(&glossary));

    let docs = collected_docs(root)?;
    diagnostics.extend(validate_docs_use_native_boundary(root, &docs)?);
    diagnostics.extend(validate_web_handler_docs(root)?);
    diagnostics.extend(validate_diagnostic_messages(root)?);

    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }

    Ok(NativeBoundaryTerminologySummary {
        checked_doc_count: docs.len(),
    })
}

/// Validates web-facing internal docs avoid stale VM-handler framing.
fn validate_web_handler_docs(root: &Path) -> QualityResult<Vec<String>> {
    let mut diagnostics = Vec::new();
    for path in SELECTED_DOC_PATHS {
        let text = read_text(root, path)?;
        for forbidden in FORBIDDEN_WEB_HANDLER_DOC_FRAGMENTS {
            if text.contains(forbidden) {
                diagnostics.push(format!(
                    "`{path}` must frame dynamic handlers as an explicit migration bridge, not `{forbidden}`"
                ));
            }
        }
    }
    Ok(diagnostics)
}

/// Validates the compatibility glossary text.
fn validate_glossary_text(text: &str) -> Vec<String> {
    let normalized = normalize_text(text);
    let mut diagnostics = Vec::new();
    for term in REQUIRED_GLOSSARY_TERMS {
        if !normalized.contains(&normalize_text(term)) {
            diagnostics.push(format!("missing native-boundary glossary term `{term}`"));
        }
    }
    for placeholder in PLACEHOLDER_TERMS {
        if normalized.contains(&normalize_text(placeholder)) {
            diagnostics.push(format!(
                "placeholder native-boundary glossary text `{placeholder}` is not allowed"
            ));
        }
    }
    diagnostics
}

/// Validates selected user-facing diagnostics use native-boundary wording.
fn validate_diagnostic_messages(root: &Path) -> QualityResult<Vec<String>> {
    let mut diagnostics = Vec::new();
    for (path, marker) in REQUIRED_DIAGNOSTIC_MARKERS {
        let text = read_text(root, path)?;
        if !text.contains(marker) {
            diagnostics.push(format!(
                "`{path}` is missing NativeBoundary diagnostic marker `{marker}`"
            ));
        }
    }
    Ok(diagnostics)
}

/// Validates NIF terminology in golden native-boundary documentation.
fn validate_docs_use_native_boundary(root: &Path, docs: &[PathBuf]) -> QualityResult<Vec<String>> {
    let mut diagnostics = Vec::new();
    for doc in docs {
        let doc_text = path_to_slash(doc);
        if doc_text == GLOSSARY_PATH {
            continue;
        }
        let text = fs::read_to_string(root.join(doc))
            .map_err(|err| format!("{}: failed to read file: {err}", doc.display()))?;
        diagnostics.extend(validate_nif_terms_for_doc(&doc_text, &text));
    }
    Ok(diagnostics)
}

/// Validates NIF wording appears only in explicit compatibility/non-goal text.
fn validate_nif_terms_for_doc(doc_text: &str, text: &str) -> Vec<String> {
    let mut diagnostics = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if !contains_nif_term(line) || nif_line_has_allowed_context(line) {
            continue;
        }
        diagnostics.push(format!(
            "`{doc_text}`:{} must not use NIF terminology unless explaining historical migration or out-of-contract behavior",
            index + 1
        ));
    }
    diagnostics
}

/// Returns whether a line contains a standalone NIF term.
fn contains_nif_term(line: &str) -> bool {
    line.split(|character: char| {
        !(character.is_ascii_alphanumeric() || character == '_' || character == '-')
    })
    .any(|word| word == "NIF" || word == "nif" || word == "NIF-era")
}

/// Returns whether a NIF line has explicit historical or non-goal context.
fn nif_line_has_allowed_context(line: &str) -> bool {
    NIF_ALLOWED_CONTEXTS
        .iter()
        .any(|allowed| line.contains(allowed))
}

/// Collects golden documentation files covered by this terminology gate.
fn collected_docs(root: &Path) -> QualityResult<Vec<PathBuf>> {
    let mut docs = Vec::new();
    for relative in DOC_ROOTS {
        collect_docs(root, Path::new(relative), &mut docs)?;
    }
    for relative in SELECTED_DOC_PATHS {
        docs.push(PathBuf::from(relative));
    }
    docs.sort();
    docs.dedup();
    Ok(docs)
}

/// Recursively collects Markdown docs under one directory.
fn collect_docs(root: &Path, relative: &Path, docs: &mut Vec<PathBuf>) -> QualityResult<()> {
    let dir = root.join(relative);
    if !dir.exists() {
        return Ok(());
    }
    for entry in
        fs::read_dir(&dir).map_err(|err| format!("{}: failed to read dir: {err}", dir.display()))?
    {
        let entry =
            entry.map_err(|err| format!("{}: failed to read dir entry: {err}", dir.display()))?;
        let child = relative.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|err| format!("{}: failed to read file type: {err}", child.display()))?;
        if file_type.is_dir() {
            collect_docs(root, &child, docs)?;
        } else if file_type.is_file()
            && child.extension().and_then(|extension| extension.to_str()) == Some("md")
        {
            docs.push(child);
        }
    }
    Ok(())
}

/// Reads a repository-relative text file.
fn read_text(root: &Path, relative: &str) -> QualityResult<String> {
    let path = root.join(relative);
    fs::read_to_string(&path)
        .map_err(|err| format!("{}: failed to read file: {err}", path.display()))
}

/// Normalizes text for term checks.
fn normalize_text(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Converts a path to slash-separated repository-relative text.
fn path_to_slash(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Renders native-boundary terminology diagnostics.
fn render_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[native-boundary-terminology] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "native_boundary_terminology_test.rs"]
#[cfg(test)]
mod native_boundary_terminology_test;
