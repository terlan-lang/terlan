use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use terlan_syntax::{
    span::Span, SyntaxDeclarationPayload, SyntaxModuleOutput, SyntaxTemplatePropOutput,
};

use super::resolve_import_path;

/// Parsed template import ready for frontend/template packaging.
///
/// Inputs:
/// - Syntax-output template import metadata plus resolved template file.
///
/// Output:
/// - Template name, source/resolved paths, props, annotation metadata, span,
///   and parsed HTML template.
///
/// Transformation:
/// - Moves template parsing, metadata extraction, and path resolution ahead of
///   package generation so artifact writers can consume validated template
///   inputs directly.
#[derive(Debug, Clone)]
pub(crate) struct SyntaxTemplateFrontendInput {
    pub(crate) name: String,
    pub(crate) source_path: String,
    pub(crate) resolved_path: PathBuf,
    pub(crate) props: Vec<SyntaxTemplatePropOutput>,
    pub(crate) metadata: terlan_html::TemplateMetadata,
    pub(crate) span: Span,
    pub(crate) parsed: terlan_html::HtmlTemplate,
}

/// Template frontend input diagnostic.
///
/// Inputs:
/// - Source span and validation message from template import collection.
///
/// Output:
/// - Error value stored beside successfully parsed template inputs.
///
/// Transformation:
/// - Keeps template import failures attached to source spans without stopping
///   collection of independent template imports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SyntaxTemplateFrontendInputError {
    pub(crate) span: Span,
    pub(crate) message: String,
}

/// Collected template frontend inputs and diagnostics.
///
/// Inputs:
/// - All template import declarations discovered in one module.
///
/// Output:
/// - Successful parsed template inputs plus recoverable import diagnostics.
///
/// Transformation:
/// - Aggregates template import collection so callers can decide whether to
///   fail fast or continue gathering artifact diagnostics.
#[derive(Debug, Clone)]
pub(crate) struct SyntaxTemplateFrontendInputs {
    pub(crate) inputs: Vec<SyntaxTemplateFrontendInput>,
    pub(crate) errors: Vec<SyntaxTemplateFrontendInputError>,
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
        let metadata = match terlan_html::extract_template_metadata(&source, &resolved_path) {
            Ok(metadata) => metadata,
            Err(diagnostics) => {
                for diagnostic in diagnostics {
                    let path = diagnostic
                        .path
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| resolved_path.display().to_string());
                    errors.push(SyntaxTemplateFrontendInputError {
                        span,
                        message: format!(
                            "failed to extract template metadata `{}` from `{}`: {}",
                            name, path, diagnostic.message
                        ),
                    });
                }
                continue;
            }
        };
        if let Some(message) = validate_syntax_template_frontend_metadata(name, props, &metadata) {
            errors.push(SyntaxTemplateFrontendInputError { span, message });
            continue;
        }
        inputs.push(SyntaxTemplateFrontendInput {
            name: name.clone(),
            source_path: source_path.clone(),
            resolved_path,
            props: props.clone(),
            metadata,
            span,
            parsed,
        });
    }

    SyntaxTemplateFrontendInputs { inputs, errors }
}

/// Validates template header metadata against the source declaration.
///
/// Inputs:
/// - `name`: source-level template declaration name.
/// - `props`: source-level template declaration props.
/// - `metadata`: parsed `@template` header metadata.
///
/// Output:
/// - Optional user-facing error message.
///
/// Transformation:
/// - Treats the source declaration as the current source of truth while
///   allowing `@template.params` to opt into an equivalent file-local
///   signature. Mismatches are rejected before generated template functions
///   consume either shape.
fn validate_syntax_template_frontend_metadata(
    name: &str,
    props: &[SyntaxTemplatePropOutput],
    metadata: &terlan_html::TemplateMetadata,
) -> Option<String> {
    if let Some(metadata_name) = &metadata.name {
        if metadata_name != name {
            return Some(format!(
                "template `{name}` metadata declares name `{metadata_name}`"
            ));
        }
    }

    if metadata.params_declared {
        return validate_syntax_template_frontend_param_metadata(name, props, &metadata.params);
    }

    None
}

/// Validates annotation-backed template params against declaration props.
///
/// Inputs:
/// - `name`: source-level template declaration name.
/// - `props`: source declaration props.
/// - `params`: annotation-backed template params.
///
/// Output:
/// - Optional user-facing error message.
///
/// Transformation:
/// - Compares arity, order, prop names, and type text so `@template.params`
///   cannot drift from the formal source declaration.
fn validate_syntax_template_frontend_param_metadata(
    name: &str,
    props: &[SyntaxTemplatePropOutput],
    params: &[terlan_html::TemplateParamMetadata],
) -> Option<String> {
    if params.len() != props.len() {
        return Some(format!(
            "template `{name}` metadata declares {} params, but source declaration has {} props",
            params.len(),
            props.len()
        ));
    }

    for (index, (prop, param)) in props.iter().zip(params.iter()).enumerate() {
        if prop.name != param.name {
            return Some(format!(
                "template `{name}` metadata param {} is `{}`, but source declaration prop is `{}`",
                index + 1,
                param.name,
                prop.name
            ));
        }
        if prop.annotation.text != param.type_text {
            return Some(format!(
                "template `{name}` metadata param `{}` has type `{}`, but source declaration has `{}`",
                param.name, param.type_text, prop.annotation.text
            ));
        }
    }

    None
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
