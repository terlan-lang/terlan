use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::terlan_erlang::try_emit_core_module_to_erlang_with_syntax_bridge;
use crate::terlan_syntax::{SyntaxDeclarationPayload, SyntaxModuleOutput};

mod repl_examples;

pub(crate) use repl_examples::validate_repl_doc_examples;

/// Error returned when an intra-doc link targets a missing anchor.
///
/// Inputs:
/// - Produced by link validation over syntax-output doc blocks.
///
/// Output:
/// - Broken target text and source offset for diagnostics.
///
/// Transformation:
/// - Preserves enough source information for command-level diagnostic emission.
#[derive(Debug)]
pub(crate) struct DocLinkError {
    pub(crate) target: String,
    pub(crate) offset: usize,
}

/// Error returned when a doc code fence is malformed or unsupported.
///
/// Inputs:
/// - Produced by fence validation over syntax-output doc blocks.
///
/// Output:
/// - Diagnostic message, source offset, and source length.
///
/// Transformation:
/// - Converts local fence validation failures into command-level diagnostics.
#[derive(Debug)]
pub(crate) struct DocFenceError {
    pub(crate) message: String,
    pub(crate) offset: usize,
    pub(crate) len: usize,
}

/// Error returned when required public docs are missing.
///
/// Inputs:
/// - Produced by public declaration documentation validation.
///
/// Output:
/// - Diagnostic message, source offset, and source length.
///
/// Transformation:
/// - Carries the missing-doc location from syntax-output spans.
#[derive(Debug)]
pub(crate) struct MissingDocError {
    pub(crate) message: String,
    pub(crate) offset: usize,
    pub(crate) len: usize,
}

/// Error returned when a Terlan doctest block fails to compile.
///
/// Inputs:
/// - Produced by doctest parsing, resolving, typechecking, or emission.
///
/// Output:
/// - Diagnostic message, source offset, and source length.
///
/// Transformation:
/// - Maps compiler diagnostics for generated doctest modules back to the
///   original doc fence.
#[derive(Debug)]
pub(crate) struct DoctestError {
    pub(crate) message: String,
    pub(crate) offset: usize,
    pub(crate) len: usize,
}

/// Parsed documentation code block extracted from syntax-output doc lines.
///
/// Inputs:
/// - Produced by scanning doc strings for fenced code blocks.
///
/// Output:
/// - Fence language, body text, and source offset.
///
/// Transformation:
/// - Preserves source position while joining fenced body lines.
#[derive(Debug, PartialEq, Eq)]
struct DocCodeBlock {
    language: String,
    body: String,
    offset: usize,
}

/// Discovers Terlan source files for documentation commands.
///
/// Inputs:
/// - `input`: file or directory path passed to `terlc doc`.
///
/// Output:
/// - A sorted list of `.terl` files for directories, or the input file path.
///
/// Transformation:
/// - Delegates directory scanning to the shared Terlan source discovery helper.
pub(crate) fn doc_sources(input: &Path) -> Result<Vec<PathBuf>, String> {
    if input.is_dir() {
        crate::formal_pipeline::terlan_sources_in_dir(input)
    } else {
        Ok(vec![input.to_path_buf()])
    }
}

/// Validates intra-doc links for a syntax-output module.
///
/// Inputs:
/// - `module`: syntax-output module containing doc metadata.
/// - `source`: original source text for diagnostic offsets.
///
/// Output:
/// - `Ok(())` when all local links resolve to known anchors.
/// - `Err(DocLinkError)` for the first broken link.
///
/// Transformation:
/// - Builds module anchors, scans each doc line, and normalizes `#anchor`
///   targets before lookup.
pub(crate) fn validate_syntax_module_doc_links(
    module: &SyntaxModuleOutput,
    source: &str,
) -> Result<(), DocLinkError> {
    let anchors = syntax_module_doc_anchors(module);
    for docs in syntax_module_doc_blocks(module) {
        validate_doc_lines(docs, &anchors, source)?;
    }
    Ok(())
}

/// Validates documentation code fences for a syntax-output module.
///
/// Inputs:
/// - `module`: syntax-output module containing doc metadata.
/// - `source`: original source text for diagnostic offsets.
///
/// Output:
/// - `Ok(())` when fences are balanced and use allowed languages.
/// - `Err(DocFenceError)` for the first malformed fence.
///
/// Transformation:
/// - Scans each doc block for Markdown fences and validates opening languages.
pub(crate) fn validate_syntax_module_doc_fences(
    module: &SyntaxModuleOutput,
    source: &str,
) -> Result<(), DocFenceError> {
    for docs in syntax_module_doc_blocks(module) {
        validate_doc_fences(docs, source)?;
    }
    Ok(())
}

/// Returns all documentation blocks attached to a syntax-output module.
///
/// Inputs:
/// - `module`: syntax-output module to inspect.
///
/// Output:
/// - Borrowed doc-line slices for the module, declarations, fields, and trait
///   methods.
///
/// Transformation:
/// - Walks declarations and includes nested documentation-bearing items.
fn syntax_module_doc_blocks(module: &SyntaxModuleOutput) -> Vec<&[String]> {
    let mut blocks = vec![module.docs.as_slice()];
    for decl in &module.declarations {
        blocks.push(decl.docs.as_slice());
        match &decl.payload {
            SyntaxDeclarationPayload::Struct { fields, .. } => {
                for field in fields {
                    blocks.push(field.docs.as_slice());
                }
            }
            SyntaxDeclarationPayload::Trait { methods, .. } => {
                for method in methods {
                    blocks.push(method.docs.as_slice());
                }
            }
            SyntaxDeclarationPayload::TraitImpl { .. } => {}
            SyntaxDeclarationPayload::Import { .. }
            | SyntaxDeclarationPayload::Export { .. }
            | SyntaxDeclarationPayload::Type { .. }
            | SyntaxDeclarationPayload::Constructor { .. }
            | SyntaxDeclarationPayload::Function { .. }
            | SyntaxDeclarationPayload::Method { .. }
            | SyntaxDeclarationPayload::AnnotationSchema { .. }
            | SyntaxDeclarationPayload::Template { .. }
            | SyntaxDeclarationPayload::Config { .. }
            | SyntaxDeclarationPayload::Raw { .. } => {}
        }
    }
    blocks
}

/// Builds the set of valid local documentation anchors for a module.
///
/// Inputs:
/// - `module`: syntax-output module to inspect.
///
/// Output:
/// - Normalized anchors accepted by intra-doc link validation.
///
/// Transformation:
/// - Adds anchors for the module and public syntax-output declarations,
///   including arity aliases for functions and trait methods.
fn syntax_module_doc_anchors(module: &SyntaxModuleOutput) -> BTreeSet<String> {
    let mut anchors = BTreeSet::new();
    insert_doc_anchor(&mut anchors, &module.module_name);
    for decl in &module.declarations {
        match &decl.payload {
            SyntaxDeclarationPayload::Type { name, .. } => insert_doc_anchor(&mut anchors, name),
            SyntaxDeclarationPayload::Struct { name, fields, .. } => {
                insert_doc_anchor(&mut anchors, name);
                for field in fields {
                    insert_doc_anchor(&mut anchors, &format!("{}.{}", name, field.name));
                }
            }
            SyntaxDeclarationPayload::Function { name, params, .. } => {
                insert_doc_anchor(&mut anchors, name);
                insert_doc_anchor(&mut anchors, &format!("{}/{}", name, params.len()));
                insert_doc_anchor(&mut anchors, &format!("{}-{}", name, params.len()));
            }
            SyntaxDeclarationPayload::Method { name, params, .. } => {
                let source_arity = params.len();
                let callable_arity = source_arity + 1;
                insert_doc_anchor(&mut anchors, name);
                insert_doc_anchor(&mut anchors, &format!("{}/{}", name, source_arity));
                insert_doc_anchor(&mut anchors, &format!("{}-{}", name, source_arity));
                insert_doc_anchor(&mut anchors, &format!("{}/{}", name, callable_arity));
                insert_doc_anchor(&mut anchors, &format!("{}-{}", name, callable_arity));
            }
            SyntaxDeclarationPayload::Trait { name, methods, .. } => {
                insert_doc_anchor(&mut anchors, name);
                insert_doc_anchor(&mut anchors, "trait");
                for method in methods {
                    insert_doc_anchor(&mut anchors, &method.name);
                    insert_doc_anchor(
                        &mut anchors,
                        &format!("{}/{}", method.name, method.params.len()),
                    );
                    insert_doc_anchor(
                        &mut anchors,
                        &format!("{}-{}", method.name, method.params.len()),
                    );
                }
            }
            SyntaxDeclarationPayload::TraitImpl {
                trait_ref,
                for_type,
                methods,
                ..
            } => {
                insert_doc_anchor(
                    &mut anchors,
                    &format!("{} for {}", trait_ref.text, for_type.text),
                );
                insert_doc_anchor(&mut anchors, "impl");
                for method in methods {
                    insert_doc_anchor(&mut anchors, &method.name);
                    insert_doc_anchor(
                        &mut anchors,
                        &format!("{}/{}", method.name, method.params.len()),
                    );
                    insert_doc_anchor(
                        &mut anchors,
                        &format!("{}-{}", method.name, method.params.len()),
                    );
                }
            }
            SyntaxDeclarationPayload::Raw { raw_kind, .. } => {
                insert_doc_anchor(&mut anchors, raw_kind)
            }
            SyntaxDeclarationPayload::Template { name, .. } => {
                insert_doc_anchor(&mut anchors, name)
            }
            SyntaxDeclarationPayload::Config { name, target, .. } => {
                insert_doc_anchor(&mut anchors, name);
                insert_doc_anchor(&mut anchors, &format!("{}.{}", name, target));
            }
            SyntaxDeclarationPayload::Constructor { name, .. } => {
                insert_doc_anchor(&mut anchors, name)
            }
            SyntaxDeclarationPayload::AnnotationSchema { .. }
            | SyntaxDeclarationPayload::Import { .. }
            | SyntaxDeclarationPayload::Export { .. } => {}
        }
    }
    anchors
}

/// Inserts one normalized documentation anchor.
///
/// Inputs:
/// - `anchors`: mutable anchor set.
/// - `name`: raw anchor name.
///
/// Output:
/// - No return value.
///
/// Transformation:
/// - Normalizes the raw name and inserts it into the set.
fn insert_doc_anchor(anchors: &mut BTreeSet<String>, name: &str) {
    anchors.insert(normalize_doc_anchor(name));
}

/// Validates intra-doc links in one doc block.
///
/// Inputs:
/// - `docs`: documentation lines to scan.
/// - `anchors`: normalized valid target anchors.
/// - `source`: original source text for diagnostic offsets.
///
/// Output:
/// - `Ok(())` when links resolve, or the first broken link.
///
/// Transformation:
/// - Extracts Markdown link targets, keeps local `#` targets, normalizes them,
///   and checks membership in the anchor set.
fn validate_doc_lines(
    docs: &[String],
    anchors: &BTreeSet<String>,
    source: &str,
) -> Result<(), DocLinkError> {
    for line in docs {
        for target in intra_doc_link_targets(line) {
            let normalized = normalize_doc_anchor(target.trim_start_matches('#'));
            if !normalized.is_empty() && !anchors.contains(&normalized) {
                let offset = source.find(target).unwrap_or(0);
                return Err(DocLinkError {
                    target: target.to_string(),
                    offset,
                });
            }
        }
    }
    Ok(())
}

/// Extracts local Markdown link targets from one documentation line.
///
/// Inputs:
/// - `line`: documentation line to inspect.
///
/// Output:
/// - Borrowed targets that begin with `#`.
///
/// Transformation:
/// - Scans simple Markdown `](...)` link syntax without allocating target text.
fn intra_doc_link_targets(line: &str) -> Vec<&str> {
    let mut targets = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find("](") {
        rest = &rest[start + 2..];
        let Some(end) = rest.find(')') else {
            break;
        };
        let target = &rest[..end];
        if target.starts_with('#') {
            targets.push(target);
        }
        rest = &rest[end + 1..];
    }
    targets
}

/// Normalizes a doc anchor or link target.
///
/// Inputs:
/// - `input`: raw anchor text.
///
/// Output:
/// - Lowercase anchor text accepted by link validation.
///
/// Transformation:
/// - Keeps alphanumerics and selected punctuation, maps whitespace to `-`, and
///   drops unsupported characters.
fn normalize_doc_anchor(input: &str) -> String {
    input
        .trim()
        .trim_matches('`')
        .chars()
        .filter_map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '/' | '_' | '.' | '-') {
                Some(ch.to_ascii_lowercase())
            } else if ch.is_whitespace() {
                Some('-')
            } else {
                None
            }
        })
        .collect()
}

/// Validates code fences in one documentation block.
///
/// Inputs:
/// - `docs`: documentation lines to scan.
/// - `source`: original source text for diagnostic offsets.
///
/// Output:
/// - `Ok(())` when fences are balanced and allowed.
/// - `Err(DocFenceError)` for unsupported languages or unclosed fences.
///
/// Transformation:
/// - Tracks the current opening fence and validates only opening languages.
fn validate_doc_fences(docs: &[String], source: &str) -> Result<(), DocFenceError> {
    let mut opening_line = None;
    for line in docs {
        let trimmed = line.trim_start();
        let Some(language) = trimmed.strip_prefix("```") else {
            continue;
        };
        if opening_line.is_some() {
            opening_line = None;
            continue;
        }
        let language = language.trim();
        if !is_allowed_doc_fence_language(language) {
            return Err(DocFenceError {
                message: format!("unsupported doc code fence language `{}`", language),
                offset: source.find(trimmed).unwrap_or(0),
                len: trimmed.len(),
            });
        }
        opening_line = Some(trimmed.to_string());
    }
    if let Some(opening_line) = opening_line {
        return Err(DocFenceError {
            message: "unclosed doc code fence".to_string(),
            offset: source.find(&opening_line).unwrap_or(0),
            len: opening_line.len(),
        });
    }
    Ok(())
}

/// Returns whether a doc fence language is accepted by the CLI.
///
/// Inputs:
/// - `language`: language tag from a Markdown fence.
///
/// Output:
/// - `true` when the language is supported.
///
/// Transformation:
/// - Checks the current allowlist used by doc validation and doctests.
fn is_allowed_doc_fence_language(language: &str) -> bool {
    matches!(language, "" | "terlan" | "erlang" | "text")
}

/// Validates that public syntax-output declarations have docs.
///
/// Inputs:
/// - `module`: syntax-output module to validate.
///
/// Output:
/// - `Ok(())` when required docs are present.
/// - `Err(MissingDocError)` for the first missing required doc block.
///
/// Transformation:
/// - Checks public declarations and public struct fields using source spans
///   already attached to syntax output.
pub(crate) fn validate_syntax_missing_docs(
    module: &SyntaxModuleOutput,
) -> Result<(), MissingDocError> {
    for decl in &module.declarations {
        match &decl.payload {
            SyntaxDeclarationPayload::Type {
                name, is_public, ..
            } => require_docs(
                *is_public,
                &decl.docs,
                format!("missing docs for public type `{}`", name),
                decl.span.start,
                decl.span.end,
            )?,
            SyntaxDeclarationPayload::Struct {
                name,
                is_public,
                fields,
                ..
            } => {
                require_docs(
                    *is_public,
                    &decl.docs,
                    format!("missing docs for public struct `{}`", name),
                    decl.span.start,
                    decl.span.end,
                )?;
                for field in fields {
                    require_docs(
                        *is_public,
                        &field.docs,
                        format!(
                            "missing docs for public struct field `{}.{}`",
                            name, field.name
                        ),
                        field.span.start,
                        field.span.end,
                    )?;
                }
            }
            SyntaxDeclarationPayload::Function {
                name,
                params,
                is_public,
                ..
            } => require_docs(
                *is_public,
                &decl.docs,
                format!(
                    "missing docs for public function `{}/{}`",
                    name,
                    params.len()
                ),
                decl.span.start,
                decl.span.end,
            )?,
            SyntaxDeclarationPayload::Method {
                name,
                params,
                is_public,
                ..
            } => require_docs(
                *is_public,
                &decl.docs,
                format!("missing docs for public method `{}/{}`", name, params.len()),
                decl.span.start,
                decl.span.end,
            )?,
            SyntaxDeclarationPayload::Trait {
                name, is_public, ..
            } => require_docs(
                *is_public,
                &decl.docs,
                format!("missing docs for public trait `{}`", name),
                decl.span.start,
                decl.span.end,
            )?,
            SyntaxDeclarationPayload::TraitImpl {
                trait_ref,
                for_type,
                is_public,
                ..
            } => require_docs(
                *is_public,
                &decl.docs,
                format!(
                    "missing docs for public impl `{} for {}`",
                    trait_ref.text, for_type.text
                ),
                decl.span.start,
                decl.span.end,
            )?,
            SyntaxDeclarationPayload::Constructor {
                name, is_public, ..
            } => require_docs(
                *is_public,
                &decl.docs,
                format!("missing docs for public constructor `{}`", name),
                decl.span.start,
                decl.span.end,
            )?,
            SyntaxDeclarationPayload::Import { .. }
            | SyntaxDeclarationPayload::Export { .. }
            | SyntaxDeclarationPayload::AnnotationSchema { .. }
            | SyntaxDeclarationPayload::Config { .. }
            | SyntaxDeclarationPayload::Raw { .. }
            | SyntaxDeclarationPayload::Template { .. } => {}
        }
    }
    Ok(())
}

/// Requires a documentation block when a declaration is public.
///
/// Inputs:
/// - `required`: whether docs are required.
/// - `docs`: declaration documentation lines.
/// - `message`: diagnostic message for missing docs.
/// - `start`: source start offset.
/// - `end`: source end offset.
///
/// Output:
/// - `Ok(())` when docs are optional or present.
/// - `Err(MissingDocError)` when docs are required and absent.
///
/// Transformation:
/// - Converts missing required docs into a span-bearing error.
fn require_docs(
    required: bool,
    docs: &[String],
    message: String,
    start: usize,
    end: usize,
) -> Result<(), MissingDocError> {
    if !required || has_docs(docs) {
        return Ok(());
    }
    Err(missing_doc_error(message, start, end))
}

/// Returns whether a doc block has non-empty content.
///
/// Inputs:
/// - `docs`: documentation lines.
///
/// Output:
/// - `true` when at least one line contains non-whitespace text.
///
/// Transformation:
/// - Trims each line and checks for visible content.
fn has_docs(docs: &[String]) -> bool {
    docs.iter().any(|line| !line.trim().is_empty())
}

/// Builds a missing-doc diagnostic from a syntax-output span.
///
/// Inputs:
/// - `message`: diagnostic message.
/// - `start`: source start offset.
/// - `end`: source end offset.
///
/// Output:
/// - Missing-doc error with a non-zero diagnostic length.
///
/// Transformation:
/// - Uses saturating subtraction and a minimum length of one.
fn missing_doc_error(message: String, start: usize, end: usize) -> MissingDocError {
    MissingDocError {
        message,
        offset: start,
        len: end.saturating_sub(start).max(1),
    }
}

/// Compiles all Terlan doctest fences in a syntax-output module.
///
/// Inputs:
/// - `module`: syntax-output module containing doc metadata.
/// - `source`: original source text for doctest fence offsets.
/// - `path`: source path used to load neighboring interfaces.
///
/// Output:
/// - `Ok(())` when all Terlan doctests parse, resolve, typecheck, and emit.
/// - `Err(DoctestError)` for the first failing doctest.
///
/// Transformation:
/// - Wraps each Terlan code fence in a temporary module when needed, then runs
///   the formal syntax-output compiler path through emission.
pub(crate) fn compile_syntax_terlan_doctests(
    module: &SyntaxModuleOutput,
    source: &str,
    path: &str,
) -> Result<(), DoctestError> {
    let interfaces = crate::terlan_hir::load_interfaces_from_file_set(path);
    let interface_map = interfaces
        .iter()
        .map(|(name, interface)| (name.clone(), interface.clone()))
        .collect::<BTreeMap<_, _>>();
    for (index, block) in syntax_module_doc_code_blocks(module, source)
        .into_iter()
        .filter(|block| block.language == "terlan")
        .enumerate()
    {
        let body =
            doctest_body_without_expected_markers(&block.body).map_err(|message| DoctestError {
                message,
                offset: block.offset,
                len: block.body.len().max(1),
            })?;
        let temp_source = temporary_doctest_module_source(&module.module_name, index, &body);
        let compile = crate::formal_pipeline::compile_syntax_module_through_phases_with_diagnostics_for_profile(
            path,
            &temp_source,
            crate::DiagnosticFormat::default(),
            None,
            crate::validation::native_policy::NativePolicy::SafeNativeOptional,
            crate::validation::target_profile::TargetProfile::Erlang,
        );
        if !compile.parse_diagnostics.is_empty() {
            return Err(DoctestError {
                message: format!(
                    "doctest parse error: {}",
                    compile
                        .parse_diagnostics
                        .first()
                        .map(|diag| diag.message.as_str())
                        .unwrap_or("failed to parse doctest")
                ),
                offset: block.offset,
                len: block.body.len().max(1),
            });
        }
        if !compile.include_expansion_diagnostics.is_empty() {
            return Err(DoctestError {
                message: format!(
                    "doctest include expansion error: {}",
                    compile
                        .include_expansion_diagnostics
                        .first()
                        .map(|diag| diag.message.as_str())
                        .unwrap_or("failed to expand includes")
                ),
                offset: block.offset,
                len: block.body.len().max(1),
            });
        }
        if let Some(error) = compile
            .resolve_diagnostics
            .iter()
            .find(|diag| diag.severity == "error")
        {
            return Err(DoctestError {
                message: format!("doctest resolve error: {}", error.message),
                offset: block.offset,
                len: block.body.len().max(1),
            });
        }
        if let Some(error) = compile
            .typecheck_diagnostics
            .iter()
            .find(|diag| diag.severity == "error")
        {
            return Err(DoctestError {
                message: format!("doctest type error: {}", error.message),
                offset: block.offset,
                len: block.body.len().max(1),
            });
        }
        if let Some(error) = compile
            .macro_expansion_diagnostics
            .iter()
            .find(|diag| diag.severity == "error")
        {
            return Err(DoctestError {
                message: format!("doctest macro expansion error: {}", error.message),
                offset: block.offset,
                len: block.body.len().max(1),
            });
        }
        let compiled = match compile.artifacts {
            Some(artifacts) => artifacts,
            None => {
                return Err(DoctestError {
                    message: "doctest compile failed".to_string(),
                    offset: block.offset,
                    len: block.body.len().max(1),
                })
            }
        };
        try_emit_core_module_to_erlang_with_syntax_bridge(
            &compiled.core,
            &compiled.syntax_output,
            &interface_map,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .map_err(|message| DoctestError {
            message: format!("doctest emit error: {}", message),
            offset: block.offset,
            len: block.body.len().max(1),
        })?;
    }
    Ok(())
}

/// Removes supported expected-output markers from a doctest body.
///
/// Inputs:
/// - `body`: raw Terlan code-fence body.
///
/// Output:
/// - Body text without `% expect:` lines, or an error for empty markers.
///
/// Transformation:
/// - Drops expectation marker lines while preserving all compile-input lines.
fn doctest_body_without_expected_markers(body: &str) -> Result<String, String> {
    let mut lines = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(expected) = trimmed.strip_prefix("% expect:") {
            if expected.trim().is_empty() {
                return Err("empty doctest expected output marker".to_string());
            }
            continue;
        }
        lines.push(line);
    }
    Ok(lines.join("\n"))
}

/// Extracts all fenced code blocks from a module's documentation.
///
/// Inputs:
/// - `module`: syntax-output module containing doc metadata.
/// - `source`: original source text for code-block offsets.
///
/// Output:
/// - Parsed code blocks from module, declaration, field, and method docs.
///
/// Transformation:
/// - Reuses doc block discovery and extends a flat list with parsed fences.
fn syntax_module_doc_code_blocks(module: &SyntaxModuleOutput, source: &str) -> Vec<DocCodeBlock> {
    let mut blocks = Vec::new();
    for docs in syntax_module_doc_blocks(module) {
        blocks.extend(doc_code_blocks(docs, source));
    }
    blocks
}

/// Extracts fenced code blocks from one doc block.
///
/// Inputs:
/// - `docs`: documentation lines.
/// - `source`: original source text for opening-fence offsets.
///
/// Output:
/// - Parsed code blocks with language, joined body, and source offset.
///
/// Transformation:
/// - Tracks one active fence and closes it on the next fence delimiter.
fn doc_code_blocks(docs: &[String], source: &str) -> Vec<DocCodeBlock> {
    let mut blocks = Vec::new();
    let mut active: Option<(String, Vec<String>, usize)> = None;
    for line in docs {
        let trimmed = line.trim_start();
        if let Some(language) = trimmed.strip_prefix("```") {
            if let Some((language, lines, offset)) = active.take() {
                blocks.push(DocCodeBlock {
                    language,
                    body: lines.join("\n"),
                    offset,
                });
            } else {
                active = Some((
                    language.trim().to_string(),
                    Vec::new(),
                    source.find(trimmed).unwrap_or(0),
                ));
            }
        } else if let Some((_, lines, _)) = active.as_mut() {
            lines.push(line.to_string());
        }
    }
    blocks
}

/// Builds source text for a temporary doctest module.
///
/// Inputs:
/// - `module_name`: original module name used as the doctest module prefix.
/// - `index`: doctest index in the source module.
/// - `body`: doctest body after marker removal.
///
/// Output:
/// - Complete Terlan source text for compilation.
///
/// Transformation:
/// - Leaves full `module ...` doctests intact and wraps expression snippets in a
///   generated `run/0` function.
fn temporary_doctest_module_source(module_name: &str, index: usize, body: &str) -> String {
    let body = body.trim();
    if body.starts_with("module ") {
        format!("{}\n", body)
    } else {
        format!(
            "module {}_doctest_{}.\n\npub run(): Dynamic ->\n    {}.\n",
            doctest_module_name(module_name),
            index,
            body
        )
    }
}

/// Normalizes a module name for generated doctest modules.
///
/// Inputs:
/// - `module_name`: source module name.
///
/// Output:
/// - Lowercase alphanumeric/underscore module-name component.
///
/// Transformation:
/// - Lowercases alphanumerics, maps other characters to underscores, and falls
///   back to `doctest` for empty names.
fn doctest_module_name(module_name: &str) -> String {
    let normalized: String = module_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    if normalized.is_empty() {
        "doctest".to_string()
    } else {
        normalized
    }
}

#[cfg(test)]
#[path = "validation_test.rs"]
mod validation_test;
