use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use terlan_hir::ModuleInterface;
use terlan_syntax::{
    span::Span, syntax_contract_identity_from_fingerprint, SyntaxContractIdentity,
    SyntaxDeclarationPayload, SyntaxImportKind, SyntaxModuleOutput, SyntaxTemplatePropOutput,
};

#[derive(Debug, Clone)]
pub(crate) struct SyntaxTemplateFrontendInput {
    pub(crate) name: String,
    pub(crate) source_path: String,
    pub(crate) resolved_path: PathBuf,
    pub(crate) props: Vec<SyntaxTemplatePropOutput>,
    pub(crate) span: Span,
    pub(crate) parsed: terlan_html::HtmlTemplate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SyntaxTemplateFrontendInputError {
    pub(crate) span: Span,
    pub(crate) message: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SyntaxTemplateFrontendInputs {
    pub(crate) inputs: Vec<SyntaxTemplateFrontendInput>,
    pub(crate) errors: Vec<SyntaxTemplateFrontendInputError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DependencyManifest {
    pub(crate) module: String,
    pub(crate) syntax_contract_identity: SyntaxContractIdentity,
    pub(crate) source_hash: u64,
    pub(crate) interface_hash: u64,
    pub(crate) interface_doc_hash: u64,
    pub(crate) dependencies: Vec<(String, u64)>,
}

impl DependencyManifest {
    /// Encodes a dependency manifest to a line-oriented cache file format.
    ///
    /// Inputs:
    /// - `self`: manifest fields produced after a successful formal compile.
    ///
    /// Output:
    /// - Text suitable for writing to a `.deps` cache artifact.
    ///
    /// Transformation:
    /// - Serializes module identity, syntax contract identity, artifact hashes,
    ///   and dependency hashes into deterministic key/value lines.
    pub(crate) fn encode(&self) -> String {
        let mut text = String::new();
        text.push_str(&format!("module={}\n", self.module));
        text.push_str(&format!(
            "syntax_contract_schema={}\n",
            self.syntax_contract_identity.schema
        ));
        text.push_str(&format!(
            "syntax_contract_fingerprint_algorithm={}\n",
            self.syntax_contract_identity.fingerprint_algorithm
        ));
        text.push_str(&format!(
            "syntax_contract_fingerprint={}\n",
            self.syntax_contract_identity.fingerprint
        ));
        text.push_str(&format!("source_hash={}\n", self.source_hash));
        text.push_str(&format!("interface_hash={}\n", self.interface_hash));
        text.push_str(&format!("interface_doc_hash={}\n", self.interface_doc_hash));
        text.push_str(&format!("deps={}\n", self.dependencies.len()));
        for (module_name, hash) in &self.dependencies {
            text.push_str(&format!("{}={}\n", module_name, hash));
        }
        text
    }

    /// Decodes a dependency manifest from a line-oriented cache file format.
    ///
    /// Inputs:
    /// - `contents`: manifest text previously produced by `encode`.
    ///
    /// Output:
    /// - Parsed manifest, or `None` when required fields are missing or invalid.
    ///
    /// Transformation:
    /// - Reads key/value lines, restores syntax contract identity metadata, and
    ///   reconstructs the dependency hash list.
    pub(crate) fn decode(contents: &str) -> Option<Self> {
        let mut module = None;
        let mut syntax_contract_schema = None;
        let mut syntax_contract_fingerprint_algorithm = None;
        let mut syntax_contract_fingerprint = None;
        let mut source_hash = None;
        let mut interface_hash = None;
        let mut interface_doc_hash = None;
        let mut deps_expected = None;
        let mut dependencies = Vec::new();
        let mut lines = contents.lines();

        for line in lines.by_ref() {
            if let Some(value) = line.strip_prefix("module=") {
                module = Some(value.to_string());
                continue;
            }
            if let Some(value) = line.strip_prefix("syntax_contract_schema=") {
                syntax_contract_schema = Some(value.to_string());
                continue;
            }
            if let Some(value) = line.strip_prefix("syntax_contract_fingerprint_algorithm=") {
                syntax_contract_fingerprint_algorithm = Some(value.to_string());
                continue;
            }
            if let Some(value) = line.strip_prefix("syntax_contract_fingerprint=") {
                syntax_contract_fingerprint = Some(value.to_string());
                continue;
            }
            if let Some(value) = line.strip_prefix("source_hash=") {
                source_hash = value.parse::<u64>().ok();
                continue;
            }
            if let Some(value) = line.strip_prefix("interface_hash=") {
                interface_hash = value.parse::<u64>().ok();
                continue;
            }
            if let Some(value) = line.strip_prefix("interface_doc_hash=") {
                interface_doc_hash = value.parse::<u64>().ok();
                continue;
            }
            if let Some(value) = line.strip_prefix("deps=") {
                deps_expected = value.parse::<usize>().ok();
                break;
            }
            return None;
        }

        for line in lines.by_ref().take(deps_expected.unwrap_or(0)) {
            let (name, hash_text) = line.split_once('=')?;
            let hash = hash_text.parse::<u64>().ok()?;
            dependencies.push((name.to_string(), hash));
        }

        let mut syntax_contract_identity =
            syntax_contract_identity_from_fingerprint(syntax_contract_fingerprint?);
        if let Some(schema) = syntax_contract_schema {
            syntax_contract_identity.schema = schema;
        }
        if let Some(fingerprint_algorithm) = syntax_contract_fingerprint_algorithm {
            syntax_contract_identity.fingerprint_algorithm = fingerprint_algorithm;
        }

        Some(Self {
            module: module?,
            syntax_contract_identity,
            source_hash: source_hash?,
            interface_hash: interface_hash?,
            interface_doc_hash: interface_doc_hash?,
            dependencies,
        })
    }

    /// Returns whether dependents should be rechecked against a prior manifest.
    ///
    /// Inputs:
    /// - `self`: current dependency manifest.
    /// - `previous`: prior dependency manifest from the cache.
    ///
    /// Output:
    /// - `true` when downstream modules must be rechecked.
    ///
    /// Transformation:
    /// - Compares syntax contract identity, emitted interface hash, and resolved
    ///   dependency hashes while ignoring source-only changes.
    pub(crate) fn should_recheck_dependents(&self, previous: &DependencyManifest) -> bool {
        self.syntax_contract_identity != previous.syntax_contract_identity
            || self.interface_hash != previous.interface_hash
            || self.dependencies != previous.dependencies
    }
}

/// Reads and decodes a dependency manifest from disk.
///
/// Inputs:
/// - `path`: manifest file path.
///
/// Output:
/// - Parsed manifest, or `None` when the file cannot be read or decoded.
///
/// Transformation:
/// - Loads UTF-8 text and delegates manifest parsing to `DependencyManifest`.
pub(crate) fn read_manifest(path: &Path) -> Option<DependencyManifest> {
    fs::read_to_string(path)
        .ok()
        .and_then(|contents| DependencyManifest::decode(&contents))
}

/// Collects dependency fingerprints for a compiled syntax module.
///
/// Inputs:
/// - `module`: formal syntax output for the source module.
/// - `interfaces`: resolved module interfaces available to the command.
/// - `source_path`: optional source path used to resolve file/template imports.
/// - `file_imports`: optional preloaded file import bytes keyed by alias.
///
/// Output:
/// - Sorted dependency name/hash pairs.
///
/// Transformation:
/// - Hashes imported module interfaces, file/CSS/Markdown imports, and external
///   template files so cache invalidation can detect semantic input changes.
pub(crate) fn collect_syntax_dependency_hashes(
    module: &SyntaxModuleOutput,
    interfaces: &HashMap<String, ModuleInterface>,
    source_path: Option<&Path>,
    file_imports: Option<&BTreeMap<String, Vec<u8>>>,
) -> Vec<(String, u64)> {
    let mut import_modules = BTreeSet::new();
    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Import {
            import_kind,
            module_name,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        if *import_kind == SyntaxImportKind::Module {
            import_modules.insert(module_name.clone());
        }
    }

    let mut out = Vec::new();
    for module_name in import_modules {
        if let Some(interface) = interfaces.get(&module_name) {
            let interface_text = interface.to_terlan_interface_type_text();
            out.push((module_name, fingerprint(interface_text.as_bytes())));
        }
    }

    if let Some(source_path) = source_path {
        let base_dir = source_path.parent().unwrap_or_else(|| Path::new("."));
        for declaration in &module.declarations {
            let SyntaxDeclarationPayload::Import {
                import_kind,
                items,
                source_path,
                ..
            } = &declaration.payload
            else {
                continue;
            };
            if !matches!(
                import_kind,
                SyntaxImportKind::File | SyntaxImportKind::Css | SyntaxImportKind::Markdown
            ) {
                continue;
            }
            let Some(alias) = items.first().map(|item| item.name.as_str()) else {
                continue;
            };
            let Some(source) = source_path.as_deref() else {
                continue;
            };
            let resolved_path = resolve_import_path(base_dir, source);
            let bytes = file_imports
                .and_then(|imports| imports.get(alias).cloned())
                .or_else(|| fs::read(&resolved_path).ok());
            if let Some(bytes) = bytes {
                out.push((
                    format!("file:{}", resolved_path.to_string_lossy()),
                    fingerprint(&bytes),
                ));
            }
        }

        for declaration in &module.declarations {
            let SyntaxDeclarationPayload::Template { source_path, .. } = &declaration.payload
            else {
                continue;
            };
            let resolved_path = resolve_import_path(base_dir, source_path);
            if let Ok(bytes) = fs::read(&resolved_path) {
                out.push((
                    format!("template:{}", resolved_path.to_string_lossy()),
                    fingerprint(&bytes),
                ));
            }
        }
    }
    out.sort();
    out
}

/// Loads file and CSS imports declared by a syntax module.
///
/// Inputs:
/// - `module`: formal syntax output containing import declarations.
/// - `source_path`: source file path used as the relative import base.
///
/// Output:
/// - Imported bytes keyed by import alias, or a user-facing error string.
///
/// Transformation:
/// - Resolves source-relative paths, reads imported files, validates CSS imports
///   as UTF-8 and syntactically valid CSS, and returns raw bytes for emission.
pub(crate) fn collect_syntax_file_import_bytes(
    module: &SyntaxModuleOutput,
    source_path: &Path,
) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let base_dir = source_path.parent().unwrap_or_else(|| Path::new("."));
    let mut imports = BTreeMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Import {
            import_kind,
            items,
            source_path,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        if !matches!(import_kind, SyntaxImportKind::File | SyntaxImportKind::Css) {
            continue;
        }

        let alias = items
            .first()
            .map(|item| item.name.as_str())
            .ok_or_else(|| "file import is missing an alias".to_string())?;
        let source = source_path
            .as_deref()
            .ok_or_else(|| format!("file import `{}` is missing a source path", alias))?;
        let resolved_path = resolve_import_path(base_dir, source);
        let bytes = fs::read(&resolved_path).map_err(|err| {
            format!(
                "failed to read imported file `{}` for `{}`: {}",
                resolved_path.display(),
                alias,
                err
            )
        })?;
        if *import_kind == SyntaxImportKind::Css {
            let source = std::str::from_utf8(&bytes).map_err(|err| {
                format!(
                    "imported CSS file `{}` for `{}` must be valid UTF-8: {}",
                    resolved_path.display(),
                    alias,
                    err
                )
            })?;
            terlan_html::validate_css(source, &resolved_path).map_err(|diagnostics| {
                diagnostics
                    .into_iter()
                    .map(|diagnostic| {
                        let path = diagnostic
                            .path
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| resolved_path.display().to_string());
                        format!("{path}: {}", diagnostic.message)
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })?;
        }
        imports.insert(alias.to_string(), bytes);
    }

    Ok(imports)
}

/// Loads and parses Markdown imports declared by a syntax module.
///
/// Inputs:
/// - `module`: formal syntax output containing Markdown import declarations.
/// - `source_path`: source file path used as the relative import base.
///
/// Output:
/// - Parsed Markdown documents keyed by import alias, or a user-facing error.
///
/// Transformation:
/// - Resolves source-relative Markdown paths, enforces UTF-8, parses Markdown,
///   and normalizes parser diagnostics into command-ready error text.
pub(crate) fn collect_syntax_markdown_inputs(
    module: &SyntaxModuleOutput,
    source_path: &Path,
) -> Result<BTreeMap<String, terlan_html::MarkdownDocument>, String> {
    let base_dir = source_path.parent().unwrap_or_else(|| Path::new("."));
    let mut imports = BTreeMap::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Import {
            import_kind,
            items,
            source_path,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        if *import_kind != SyntaxImportKind::Markdown {
            continue;
        }

        let alias = items
            .first()
            .map(|item| item.name.as_str())
            .ok_or_else(|| "markdown import is missing an alias".to_string())?;
        let source = source_path
            .as_deref()
            .ok_or_else(|| format!("markdown import `{}` is missing a source path", alias))?;
        let resolved_path = resolve_import_path(base_dir, source);
        let bytes = fs::read(&resolved_path).map_err(|err| {
            format!(
                "failed to read imported markdown file `{}` for `{}`: {}",
                resolved_path.display(),
                alias,
                err
            )
        })?;
        let source = std::str::from_utf8(&bytes).map_err(|err| {
            format!(
                "imported markdown file `{}` for `{}` must be valid UTF-8: {}",
                resolved_path.display(),
                alias,
                err
            )
        })?;
        let document =
            terlan_html::parse_markdown(source, &resolved_path).map_err(|diagnostics| {
                diagnostics
                    .into_iter()
                    .map(|diagnostic| {
                        let path = diagnostic
                            .path
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| resolved_path.display().to_string());
                        format!("{path}: {}", diagnostic.message)
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })?;
        imports.insert(alias.to_string(), document);
    }

    Ok(imports)
}

/// Loads and parses normalized external template frontend inputs.
///
/// Inputs:
/// - `module`: formal syntax output containing template declarations.
/// - `source_path`: source file path used as the relative template base.
///
/// Output:
/// - Parsed template frontend inputs plus per-declaration errors.
///
/// Transformation:
/// - Resolves source-relative template paths, reads template source, parses it,
///   and preserves declaration props and spans for later validation phases.
pub(crate) fn collect_syntax_template_frontend_inputs(
    module: &SyntaxModuleOutput,
    source_path: &Path,
) -> SyntaxTemplateFrontendInputs {
    let base_dir = source_path.parent().unwrap_or_else(|| Path::new("."));
    let mut inputs = Vec::new();
    let mut errors = Vec::new();

    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Template {
            name,
            source_path,
            props,
        } = &declaration.payload
        else {
            continue;
        };

        let resolved_path = resolve_import_path(base_dir, source_path);
        let span = declaration.span.into();
        let source = match fs::read_to_string(&resolved_path) {
            Ok(source) => source,
            Err(err) => {
                errors.push(SyntaxTemplateFrontendInputError {
                    span,
                    message: format!(
                        "failed to read template `{}` for `{}`: {}",
                        resolved_path.display(),
                        name,
                        err
                    ),
                });
                continue;
            }
        };
        let parsed = match terlan_html::parse_template(&source, &resolved_path) {
            Ok(parsed) => parsed,
            Err(diagnostics) => {
                for diagnostic in diagnostics {
                    let path = diagnostic
                        .path
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| resolved_path.display().to_string());
                    errors.push(SyntaxTemplateFrontendInputError {
                        span,
                        message: format!(
                            "failed to parse template `{}` from `{}`: {}",
                            name, path, diagnostic.message
                        ),
                    });
                }
                continue;
            }
        };
        inputs.push(SyntaxTemplateFrontendInput {
            name: name.clone(),
            source_path: source_path.clone(),
            resolved_path,
            props: props.clone(),
            span,
            parsed,
        });
    }

    SyntaxTemplateFrontendInputs { inputs, errors }
}

/// Loads and parses external template declarations from a syntax module.
///
/// Inputs:
/// - `module`: formal syntax output containing template declarations.
/// - `source_path`: source file path used as the relative template base.
///
/// Output:
/// - Parsed HTML templates keyed by template name, or a user-facing error.
///
/// Transformation:
/// - Uses the normalized template frontend collector and converts any
///   frontend diagnostics into command-ready error text.
pub(crate) fn collect_syntax_template_inputs(
    module: &SyntaxModuleOutput,
    source_path: &Path,
) -> Result<BTreeMap<String, terlan_html::HtmlTemplate>, String> {
    let collected = collect_syntax_template_frontend_inputs(module, source_path);
    if !collected.errors.is_empty() {
        return Err(collected
            .errors
            .into_iter()
            .map(|error| error.message)
            .collect::<Vec<_>>()
            .join("\n"));
    }

    let mut templates = BTreeMap::new();
    for input in collected.inputs {
        templates.insert(input.name, input.parsed);
    }

    Ok(templates)
}

/// Resolves an import or template source path relative to a source directory.
///
/// Inputs:
/// - `base_dir`: directory of the Terlan source file.
/// - `source`: import or template path from syntax output.
///
/// Output:
/// - Absolute paths are returned unchanged; relative paths are joined to
///   `base_dir`.
///
/// Transformation:
/// - Converts source-level path text into a filesystem path for command IO.
pub(crate) fn resolve_import_path(base_dir: &Path, source: &str) -> PathBuf {
    let path = Path::new(source);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}

/// Computes a local deterministic hash for CLI artifact invalidation.
///
/// Inputs:
/// - `bytes`: content bytes to fingerprint.
///
/// Output:
/// - A `u64` hash value used inside CLI cache artifacts.
///
/// Transformation:
/// - Feeds bytes into Rust's default hasher for compact cache comparisons.
pub(crate) fn fingerprint(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use terlan_syntax::parse_module_as_syntax_output;

    use super::collect_syntax_template_frontend_inputs;

    /// Builds a unique temporary directory for artifact tests.
    ///
    /// Inputs:
    /// - `name`: stable test-name prefix.
    ///
    /// Output:
    /// - A path under the system temporary directory.
    ///
    /// Transformation:
    /// - Combines the current process id and wall-clock nanoseconds so tests
    ///   can run repeatedly without reusing previous fixture directories.
    fn temp_artifact_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("terlan-{name}-{}-{unique}", std::process::id()))
    }

    /// Proves the A0.50 template frontend collector preserves normalized input.
    ///
    /// Inputs:
    /// - No external input; the test creates a temporary Terlan source file and
    ///   sibling template file.
    ///
    /// Output:
    /// - Test assertion result.
    ///
    /// Transformation:
    /// - Parses syntax output, resolves and parses the declared template file,
    ///   then checks preserved declaration metadata and parsed HTML metadata.
    #[test]
    fn collect_syntax_template_frontend_inputs_preserves_normalized_template_metadata() {
        let dir = temp_artifact_dir("template-frontend-input");
        fs::create_dir_all(&dir).expect("create temp artifact dir");
        let source_path = dir.join("page_test.tl");
        let template_path = dir.join("page.tl.html");
        fs::write(
            &source_path,
            r#"module page_test.

template Page from "page.tl.html" {
  title: String
}.
"#,
        )
        .expect("write source fixture");
        fs::write(
            &template_path,
            r#"<template tag="page-view"><h1>{title}</h1></template>"#,
        )
        .expect("write template fixture");

        let source = fs::read_to_string(&source_path).expect("read source fixture");
        let module = parse_module_as_syntax_output(&source).expect("parse source fixture");
        let collected = collect_syntax_template_frontend_inputs(&module, &source_path);

        assert!(collected.errors.is_empty(), "{:?}", collected.errors);
        assert_eq!(collected.inputs.len(), 1);
        let input = &collected.inputs[0];
        assert_eq!(input.name, "Page");
        assert_eq!(input.source_path, "page.tl.html");
        assert_eq!(input.resolved_path, template_path);
        assert_eq!(input.props.len(), 1);
        assert_eq!(input.props[0].name, "title");
        assert_eq!(input.props[0].annotation.text, "String");
        assert!(input.span.end > input.span.start);
        assert_eq!(input.parsed.tag_name.as_deref(), Some("page"));

        fs::remove_dir_all(&dir).expect("remove temp artifact dir");
    }
}
