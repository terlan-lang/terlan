use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::terlan_hir::ModuleInterface;
use crate::terlan_syntax::{
    syntax_contract_identity_from_fingerprint, SyntaxContractIdentity, SyntaxDeclarationPayload,
    SyntaxImportKind, SyntaxModuleOutput,
};

#[path = "artifacts/templates.rs"]
mod templates;

pub(crate) use templates::{
    collect_syntax_template_frontend_inputs, collect_syntax_template_inputs,
    SyntaxTemplateFrontendInput,
};

/// Validated source asset import ready for command-owned packaging.
///
/// Inputs:
/// - Produced from syntax-output `import file/css/markdown` declarations.
///
/// Output:
/// - Import alias, source kind, source path text, resolved filesystem path, and
///   raw bytes for downstream artifact writers.
///
/// Transformation:
/// - Preserves source-level asset import metadata after path resolution and
///   validation so packaging commands do not have to re-scan source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SyntaxAssetImportInput {
    pub(crate) alias: String,
    pub(crate) kind: SyntaxImportKind,
    pub(crate) source_path: String,
    pub(crate) resolved_path: PathBuf,
    pub(crate) bytes: Vec<u8>,
}

/// Parsed Markdown import ready for static rendering and route discovery.
///
/// Inputs:
/// - Syntax-output Markdown import metadata plus the resolved file.
///
/// Output:
/// - Import alias, source/resolved paths, parsed document, and page metadata.
///
/// Transformation:
/// - Preserves source-level import metadata beside parsed Markdown so static
///   profile route discovery can consume `@page` without reparsing imports.
#[derive(Debug, Clone)]
pub(crate) struct SyntaxMarkdownInput {
    pub(crate) alias: String,
    pub(crate) source_path: String,
    pub(crate) resolved_path: PathBuf,
    pub(crate) metadata: crate::terlan_html::PageMetadata,
    pub(crate) document: crate::terlan_html::MarkdownDocument,
}

/// Deterministic dependency manifest for incremental builds.
///
/// Inputs:
/// - Module identity, syntax contract identity, source/interface hashes, and
///   dependency hashes.
///
/// Output:
/// - Encodable manifest used to decide whether cached artifacts are current.
///
/// Transformation:
/// - Records only stable identity and hash data so incremental checks avoid
///   loading or comparing full dependency source text.
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
            crate::terlan_html::validate_css(source, &resolved_path).map_err(|diagnostics| {
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

/// Loads source asset imports declared by a syntax module.
///
/// Inputs:
/// - `module`: formal syntax output containing asset import declarations.
/// - `source_path`: source file path used as the relative import base.
///
/// Output:
/// - Imported asset metadata and bytes, or a user-facing error string.
///
/// Transformation:
/// - Resolves source-relative import paths, reads file/CSS/Markdown assets,
///   preserves import alias and kind metadata, validates CSS syntax, validates
///   Markdown as UTF-8 parseable input, validates artifact-template assets, and
///   returns raw bytes for packaging.
pub(crate) fn collect_syntax_asset_imports(
    module: &SyntaxModuleOutput,
    source_path: &Path,
) -> Result<Vec<SyntaxAssetImportInput>, String> {
    collect_syntax_asset_imports_matching(module, source_path, |_, _| true)
}

/// Loads asset imports declared by a syntax module when a filter accepts them.
///
/// Inputs:
/// - `module`: formal syntax output containing import declarations.
/// - `source_path`: source file path used as the relative import base.
/// - `include`: predicate receiving import kind and resolved path.
///
/// Output:
/// - Imported asset metadata and bytes for accepted imports, or a user-facing
///   error string.
///
/// Transformation:
/// - Resolves source-relative import paths, applies the caller filter, reads
///   accepted file/CSS/Markdown assets, validates their target-specific shape,
///   and returns raw bytes for packaging.
pub(crate) fn collect_syntax_asset_imports_matching(
    module: &SyntaxModuleOutput,
    source_path: &Path,
    include: impl Fn(SyntaxImportKind, &Path) -> bool,
) -> Result<Vec<SyntaxAssetImportInput>, String> {
    let base_dir = source_path.parent().unwrap_or_else(|| Path::new("."));
    let mut imports = Vec::new();

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

        let alias = items
            .first()
            .map(|item| item.name.as_str())
            .ok_or_else(|| "asset import is missing an alias".to_string())?;
        let source = source_path
            .as_deref()
            .ok_or_else(|| format!("asset import `{}` is missing a source path", alias))?;
        let resolved_path = resolve_import_path(base_dir, source);
        if !include(*import_kind, &resolved_path) {
            continue;
        }
        let bytes = fs::read(&resolved_path).map_err(|err| {
            format!(
                "failed to read imported asset `{}` for `{}`: {}",
                resolved_path.display(),
                alias,
                err
            )
        })?;

        match import_kind {
            SyntaxImportKind::Css => validate_imported_css(alias, &resolved_path, &bytes)?,
            SyntaxImportKind::Markdown => {
                validate_imported_markdown(alias, &resolved_path, &bytes)?;
            }
            SyntaxImportKind::File => {
                validate_imported_artifact_template(alias, &resolved_path, &bytes)?;
            }
            SyntaxImportKind::Module => {}
        }

        imports.push(SyntaxAssetImportInput {
            alias: alias.to_string(),
            kind: *import_kind,
            source_path: source.to_string(),
            resolved_path,
            bytes,
        });
    }

    Ok(imports)
}

/// Validates one imported artifact-template asset when the suffix requires it.
///
/// Inputs:
/// - `alias`: source import alias used in diagnostics.
/// - `resolved_path`: filesystem path read for the import.
/// - `bytes`: imported file bytes.
///
/// Output:
/// - `Ok(())` when the path is not a Terlan artifact template, or when the
///   template source is UTF-8 and structurally valid for its target suffix.
///
/// Transformation:
/// - Detects `.terl.<target>` assets, decodes them as UTF-8, delegates target
///   validation to `terlan_html`, and normalizes frontend diagnostics into
///   command-ready error text.
fn validate_imported_artifact_template(
    alias: &str,
    resolved_path: &Path,
    bytes: &[u8],
) -> Result<(), String> {
    if !crate::terlan_html::is_terlan_artifact_template_path(resolved_path) {
        return Ok(());
    }

    let source = std::str::from_utf8(bytes).map_err(|err| {
        format!(
            "imported artifact template `{}` for `{}` must be valid UTF-8: {}",
            resolved_path.display(),
            alias,
            err
        )
    })?;

    crate::terlan_html::validate_artifact_template_structure(source, resolved_path).map_err(
        |diagnostics| {
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
        },
    )
}

/// Validates one imported CSS asset.
///
/// Inputs:
/// - `alias`: source import alias used in diagnostics.
/// - `resolved_path`: filesystem path read for the import.
/// - `bytes`: imported file bytes.
///
/// Output:
/// - `Ok(())` when the file is UTF-8 and accepted by the CSS validator.
///
/// Transformation:
/// - Converts bytes into UTF-8 text, runs the HTML/CSS frontend validator, and
///   normalizes frontend diagnostics into command-ready error text.
fn validate_imported_css(alias: &str, resolved_path: &Path, bytes: &[u8]) -> Result<(), String> {
    let source = std::str::from_utf8(bytes).map_err(|err| {
        format!(
            "imported CSS file `{}` for `{}` must be valid UTF-8: {}",
            resolved_path.display(),
            alias,
            err
        )
    })?;
    crate::terlan_html::validate_css(source, resolved_path).map_err(|diagnostics| {
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
    })
}

/// Validates one imported Markdown asset.
///
/// Inputs:
/// - `alias`: source import alias used in diagnostics.
/// - `resolved_path`: filesystem path read for the import.
/// - `bytes`: imported file bytes.
///
/// Output:
/// - `Ok(())` when the file is UTF-8 and accepted by the Markdown parser.
///
/// Transformation:
/// - Converts bytes into UTF-8 text, parses Markdown through the frontend, and
///   normalizes parser diagnostics into command-ready error text.
fn validate_imported_markdown(
    alias: &str,
    resolved_path: &Path,
    bytes: &[u8],
) -> Result<(), String> {
    let source = std::str::from_utf8(bytes).map_err(|err| {
        format!(
            "imported markdown file `{}` for `{}` must be valid UTF-8: {}",
            resolved_path.display(),
            alias,
            err
        )
    })?;
    crate::terlan_html::parse_markdown(source, resolved_path)
        .map(|_| ())
        .map_err(|diagnostics| {
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
        })
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
) -> Result<BTreeMap<String, crate::terlan_html::MarkdownDocument>, String> {
    collect_syntax_markdown_frontend_inputs(module, source_path).map(|inputs| {
        inputs
            .into_iter()
            .map(|input| (input.alias, input.document))
            .collect()
    })
}

/// Loads Markdown imports with source metadata preserved.
///
/// Inputs:
/// - `module`: formal syntax output containing Markdown import declarations.
/// - `source_path`: source file path used as the relative import base.
///
/// Output:
/// - Parsed Markdown frontend inputs keyed by declaration order.
///
/// Transformation:
/// - Resolves source-relative Markdown paths, enforces UTF-8, parses page
///   metadata and Markdown body through `terlan_html`, and normalizes frontend
///   diagnostics into command-ready error text.
pub(crate) fn collect_syntax_markdown_frontend_inputs(
    module: &SyntaxModuleOutput,
    source_path: &Path,
) -> Result<Vec<SyntaxMarkdownInput>, String> {
    let base_dir = source_path.parent().unwrap_or_else(|| Path::new("."));
    let mut imports = Vec::new();

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
        let parsed = load_syntax_markdown_input(alias, source, &resolved_path)?;
        imports.push(parsed);
    }

    Ok(imports)
}

/// Loads one Markdown import as a frontend input.
///
/// Inputs:
/// - `alias`: import alias from source.
/// - `source_path`: source path text from the import declaration.
/// - `resolved_path`: filesystem path to read.
///
/// Output:
/// - Parsed Markdown frontend input or a user-facing error string.
///
/// Transformation:
/// - Reads UTF-8 source once, extracts `@page` metadata, parses Markdown, and
///   preserves all import identity needed by static-site route discovery.
fn load_syntax_markdown_input(
    alias: &str,
    source_path: &str,
    resolved_path: &Path,
) -> Result<SyntaxMarkdownInput, String> {
    let source = read_markdown_source(alias, resolved_path)?;
    let metadata = crate::terlan_html::extract_page_metadata(&source, resolved_path)
        .map_err(|diagnostics| format_html_diagnostics(diagnostics, resolved_path))?;
    let document = crate::terlan_html::parse_markdown(&source, resolved_path)
        .map_err(|diagnostics| format_html_diagnostics(diagnostics, resolved_path))?;

    Ok(SyntaxMarkdownInput {
        alias: alias.to_string(),
        source_path: source_path.to_string(),
        resolved_path: resolved_path.to_path_buf(),
        metadata,
        document,
    })
}

/// Reads one imported Markdown file as UTF-8 text.
///
/// Inputs:
/// - `alias`: import alias used in diagnostics.
/// - `resolved_path`: filesystem path to read.
///
/// Output:
/// - Markdown source text or a user-facing read/UTF-8 error.
///
/// Transformation:
/// - Reads raw bytes and converts them to an owned UTF-8 string.
fn read_markdown_source(alias: &str, resolved_path: &Path) -> Result<String, String> {
    let bytes = fs::read(resolved_path).map_err(|err| {
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
    Ok(source.to_string())
}

/// Formats HTML frontend diagnostics for CLI output.
///
/// Inputs:
/// - `diagnostics`: diagnostics from `terlan_html`.
/// - `fallback_path`: path used when a diagnostic has no path.
///
/// Output:
/// - Newline-joined command error text.
///
/// Transformation:
/// - Converts structured diagnostics into the existing command error format.
fn format_html_diagnostics(
    diagnostics: Vec<crate::terlan_html::HtmlDiagnostic>,
    fallback_path: &Path,
) -> String {
    diagnostics
        .into_iter()
        .map(|diagnostic| {
            let path = diagnostic
                .path
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| fallback_path.display().to_string());
            format!("{path}: {}", diagnostic.message)
        })
        .collect::<Vec<_>>()
        .join("\n")
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
#[path = "artifacts_test.rs"]
mod artifacts_test;
