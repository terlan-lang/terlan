use std::collections::{BTreeMap, BTreeSet};

use super::{SyntaxPatternKind, SyntaxPatternOutput};
use crate::terlan_syntax::ebnf::{EbnfCompileError, EbnfCompileResult};

/// Collects the capture names carried by binary layout field keys.
pub(super) fn collect_capture_bindings(
    pattern: &SyntaxPatternOutput,
    bindings: &mut BTreeSet<String>,
) {
    bindings.extend(
        pattern
            .fields
            .iter()
            .map(|field| field.key.as_str())
            .filter(|binding| *binding != "_")
            .map(str::to_string),
    );
}

/// Finds duplicate binary captures without treating descriptors as bindings.
pub(super) fn duplicate_capture_binding(
    pattern: &SyntaxPatternOutput,
    seen: &mut BTreeSet<String>,
) -> Option<String> {
    for field in &pattern.fields {
        if field.key != "_" && !seen.insert(field.key.clone()) {
            return Some(field.key.clone());
        }
    }
    None
}

/// Substitutes shape arguments into binary capture names while preserving
/// descriptor metadata exactly.
pub(super) fn substitute_captures(
    pattern: &mut SyntaxPatternOutput,
    substitutions: &BTreeMap<String, SyntaxPatternOutput>,
    shape_name: &str,
) -> EbnfCompileResult<()> {
    for field in &mut pattern.fields {
        let Some(replacement) = substitutions.get(&field.key) else {
            continue;
        };
        field.key = capture_argument_name(replacement).ok_or_else(|| {
            EbnfCompileError::Serialize(format!(
                "shape `{shape_name}` binary capture parameter `{}` requires a variable or wildcard pattern argument",
                field.key
            ))
        })?;
    }
    Ok(())
}

fn capture_argument_name(argument: &SyntaxPatternOutput) -> Option<String> {
    match argument.kind {
        SyntaxPatternKind::Var => argument.text.clone(),
        SyntaxPatternKind::Wildcard => Some("_".to_string()),
        _ => None,
    }
}
