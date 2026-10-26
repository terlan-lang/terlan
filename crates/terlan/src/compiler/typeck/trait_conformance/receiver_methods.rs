//! Receiver method ownership and declared trait conformance.

use super::*;

/// Extracts the unqualified owner type head from receiver annotation text.
///
/// Inputs:
/// - `text`: normalized receiver annotation text.
///
/// Output:
/// - The unqualified receiver type head for simple named receiver types.
/// - `None` for qualified/imported, tuple, list, map, function, or malformed
///   receiver annotations.
///
/// Transformation:
/// - Reads identifier characters up to a type-argument delimiter and rejects
///   annotations whose owner cannot be represented as a single local type name.
pub(super) fn receiver_owner_type_name_from_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.contains('.') {
        return None;
    }
    let head = trimmed
        .split(['[', ' ', '\t', '\r', '\n'])
        .next()
        .unwrap_or_default();
    if head
        .chars()
        .next()
        .map(|ch| ch.is_ascii_uppercase())
        .unwrap_or(false)
        && head
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        Some(head.to_string())
    } else {
        None
    }
}

/// Checks one required method for an `implements` conformance.
///
/// Inputs:
/// - `type_name`: type declaring the `implements` clause.
/// - `implemented_trait`: parsed trait reference from the conformance clause.
/// - `trait_signature`: declared trait type parameters.
/// - `method_name`: required trait method name.
/// - `expected_method`: trait method signature before substitution.
/// - `receiver_method`: matching receiver method, if one exists.
/// - `fallback_span`: span for diagnostics when no method-specific span exists.
/// - `diagnostics`: output diagnostic buffer.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Substitutes trait type parameters with conformance type arguments and then
///   compares the resulting method signature with the receiver-method shape:
///   the first trait method parameter maps to the receiver, and remaining
///   parameters map to ordinary method arguments.
pub(super) struct DeclaredMethodConformance<'a> {
    pub(super) type_name: &'a str,
    pub(super) implemented_trait: &'a ParsedTraitInstance,
    pub(super) trait_signature: &'a ParsedTraitSignature,
    pub(super) method_name: &'a str,
    pub(super) expected_method: &'a TraitMethodSignature,
    pub(super) receiver_method: Option<&'a ReceiverMethodSignature>,
    pub(super) fallback_span: Span,
}

pub(super) fn check_declared_implements_method(
    conformance: DeclaredMethodConformance<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let DeclaredMethodConformance {
        type_name,
        implemented_trait,
        trait_signature,
        method_name,
        expected_method,
        receiver_method,
        fallback_span,
    } = conformance;
    let specialized_params = expected_method
        .params
        .iter()
        .map(|param| {
            specialize_trait_type_text(
                &param.ty,
                &trait_signature.type_params,
                &implemented_trait.type_args,
            )
        })
        .collect::<Vec<_>>();
    let specialized_return = specialize_trait_type_text(
        &expected_method.return_type,
        &trait_signature.type_params,
        &implemented_trait.type_args,
    );

    if specialized_params.is_empty() {
        diagnostics.push(Diagnostic {
            span: fallback_span,
            message: format!(
                "trait method `{}` in `{}` must declare a receiver parameter for `implements`",
                method_name, implemented_trait.name
            ),
            severity: DiagSeverity::Error,
        });
        return;
    }

    let expected_receiver = &specialized_params[0];
    if !trait_type_text_equal(expected_receiver, type_name) {
        diagnostics.push(Diagnostic {
            span: fallback_span,
            message: format!(
                "trait method `{}` in `{}` expects receiver {}, but `{}` implements it",
                method_name, implemented_trait.name, expected_receiver, type_name
            ),
            severity: DiagSeverity::Error,
        });
        return;
    }

    let Some(receiver_method) = receiver_method else {
        if !expected_method.has_default {
            diagnostics.push(Diagnostic {
                span: fallback_span,
                message: format!(
                    "missing receiver method `{}` for `{}` implementing `{}`",
                    method_name, type_name, implemented_trait.name
                ),
                severity: DiagSeverity::Error,
            });
        }
        return;
    };

    if expected_method
        .params
        .first()
        .is_some_and(|param| param.is_mutable)
        && !receiver_method.receiver_mutable
    {
        diagnostics.push(Diagnostic {
            span: receiver_method.span,
            message: format!(
                "receiver method `{}` for `{}` must use a mutable receiver",
                method_name, type_name
            ),
            severity: DiagSeverity::Error,
        });
    }

    let expected_args = &specialized_params[1..];
    if receiver_method.params.len() != expected_args.len() {
        diagnostics.push(Diagnostic {
            span: receiver_method.span,
            message: format!(
                "receiver method `{}` for `{}` has arity {}, expected {}",
                method_name,
                type_name,
                receiver_method.params.len(),
                expected_args.len()
            ),
            severity: DiagSeverity::Error,
        });
        return;
    }

    for (idx, (expected, found)) in expected_args
        .iter()
        .zip(receiver_method.params.iter())
        .enumerate()
    {
        if expected_method
            .params
            .get(idx + 1)
            .is_some_and(|param| param.is_mutable)
            && !receiver_method
                .param_mutability
                .get(idx)
                .copied()
                .unwrap_or(false)
        {
            diagnostics.push(Diagnostic {
                span: receiver_method.span,
                message: format!(
                    "receiver method `{}` parameter {} for `{}` must be mutable",
                    method_name,
                    idx + 1,
                    type_name
                ),
                severity: DiagSeverity::Error,
            });
        }
        if !trait_type_text_equal(expected, found) {
            diagnostics.push(Diagnostic {
                span: receiver_method.span,
                message: format!(
                    "receiver method `{}` parameter {} for `{}` expects {}, found {}",
                    method_name,
                    idx + 1,
                    type_name,
                    expected,
                    found
                ),
                severity: DiagSeverity::Error,
            });
        }
    }

    if !trait_type_text_equal(&specialized_return, &receiver_method.return_type) {
        diagnostics.push(Diagnostic {
            span: receiver_method.span,
            message: format!(
                "receiver method `{}` return type for `{}` expects {}, found {}",
                method_name, type_name, specialized_return, receiver_method.return_type
            ),
            severity: DiagSeverity::Error,
        });
    }
}
