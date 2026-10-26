//! Template property and metadata signature validation.

use super::*;

/// Validates the source-level template prop signature.
///
/// Inputs:
/// - `template`: normalized template declaration with prop names and spans.
///
/// Output:
/// - Diagnostics for reserved or duplicate prop names.
///
/// Transformation:
/// - Scans prop names only; type compatibility and slot-path validation remain
///   in the template-node checks that need parsed template structure.
pub(super) fn validate_template_prop_signatures(template: &TemplateCheckDecl) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut prop_names = BTreeSet::new();

    diagnostics.extend(validate_template_metadata_signatures(template));

    for prop in &template.props {
        if prop.name == crate::template_inputs::TEMPLATE_CHILDREN_SLOT {
            diagnostics.push(Diagnostic {
                span: prop.span,
                message: format!(
                    "template `{}` declares reserved prop `{}`",
                    template.name,
                    crate::template_inputs::TEMPLATE_CHILDREN_SLOT
                ),
                severity: DiagSeverity::Error,
            });
        }
        if !prop_names.insert(prop.name.clone()) {
            diagnostics.push(Diagnostic {
                span: prop.span,
                message: format!(
                    "duplicate prop `{}` in template `{}`",
                    prop.name, template.name
                ),
                severity: DiagSeverity::Error,
            });
        }
    }

    diagnostics
}

/// Revalidates annotation-backed template signature metadata.
///
/// Inputs:
/// - `template`: normalized template declaration with parsed header metadata.
///
/// Output:
/// - Diagnostics for metadata drift from the source declaration.
///
/// Transformation:
/// - Reuses the normalized template-contract prop shape so downstream template
///   validation remains correct even when future entry points bypass artifact
///   collection's early mismatch rejection.
pub(super) fn validate_template_metadata_signatures(
    template: &TemplateCheckDecl,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if let Some(metadata_name) = &template.metadata.name {
        if metadata_name != &template.name {
            diagnostics.push(Diagnostic {
                span: template.span,
                message: format!(
                    "template `{}` metadata declares name `{}`",
                    template.name, metadata_name
                ),
                severity: DiagSeverity::Error,
            });
        }
    }

    if template.metadata.params_declared {
        diagnostics.extend(validate_template_metadata_params(template));
    }

    diagnostics
}

/// Revalidates annotation-backed template params.
///
/// Inputs:
/// - `template`: normalized template declaration with props and metadata.
///
/// Output:
/// - Diagnostics for arity, order, name, and type mismatches.
///
/// Transformation:
/// - Compares the source declaration and `@template.params` in the validator's
///   local shape so future template-function generation can trust both.
pub(super) fn validate_template_metadata_params(template: &TemplateCheckDecl) -> Vec<Diagnostic> {
    if template.metadata.params.len() != template.props.len() {
        return vec![Diagnostic {
            span: template.span,
            message: format!(
                "template `{}` metadata declares {} params, but source declaration has {} props",
                template.name,
                template.metadata.params.len(),
                template.props.len()
            ),
            severity: DiagSeverity::Error,
        }];
    }

    let mut diagnostics = Vec::new();
    for (index, (prop, param)) in template
        .props
        .iter()
        .zip(template.metadata.params.iter())
        .enumerate()
    {
        if prop.name != param.name {
            diagnostics.push(Diagnostic {
                span: template.span,
                message: format!(
                    "template `{}` metadata param {} is `{}`, but source declaration prop is `{}`",
                    template.name,
                    index + 1,
                    param.name,
                    prop.name
                ),
                severity: DiagSeverity::Error,
            });
        }
        if prop.annotation != param.type_text {
            diagnostics.push(Diagnostic {
                span: template.span,
                message: format!(
                    "template `{}` metadata param `{}` has type `{}`, but source declaration has `{}`",
                    template.name, param.name, param.type_text, prop.annotation
                ),
                severity: DiagSeverity::Error,
            });
        }
    }
    diagnostics
}
