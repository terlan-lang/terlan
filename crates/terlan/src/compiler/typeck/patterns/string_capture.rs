use std::collections::{HashMap, HashSet};

use crate::terlan_syntax::{SyntaxPatternKind, SyntaxPatternOutput};

use super::{apply_subst, expand_type_aliases, parse_type_expr, unify, Type, TypeAlias, TypeVarId};

/// Checks and binds captures inside a segmented string pattern.
pub(super) fn check_string_capture_pattern(
    pattern: &SyntaxPatternOutput,
    expected: &Type,
    aliases: &HashMap<String, TypeAlias>,
    locals: &mut HashMap<String, Type>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<(), String> {
    unify(expected, &Type::Binary, subst)?;
    let mut seen = HashSet::new();
    for child in &pattern.children {
        if child.kind != SyntaxPatternKind::StringCapture {
            return Err("string pattern children must be string captures".to_string());
        }
        let (name, ty) = string_capture_binding(child, aliases)?;
        if name.starts_with('_') {
            continue;
        }
        if !seen.insert(name.clone()) {
            return Err(format!("duplicate string capture name {name}"));
        }
        locals.insert(name, apply_subst(&ty, subst));
    }
    Ok(())
}

fn string_capture_binding(
    capture: &SyntaxPatternOutput,
    aliases: &HashMap<String, TypeAlias>,
) -> Result<(String, Type), String> {
    let text = capture.text.as_deref().unwrap_or_default();
    let (name, annotation) = match text.split_once(':') {
        Some((name, annotation)) => (name.trim(), Some(annotation.trim())),
        None => (text.trim(), None),
    };
    if name.is_empty() {
        return Err("string capture name cannot be empty".to_string());
    }
    let Some(annotation) = annotation else {
        return Ok((name.to_string(), Type::Binary));
    };
    if !string_capture_annotation_has_type_syntax(annotation) {
        return Err(format!(
            "invalid string capture type annotation `{annotation}`"
        ));
    }
    let alias_names = aliases.keys().cloned().collect::<HashSet<_>>();
    let mut vars = HashMap::new();
    let mut next_var: TypeVarId = 0;
    let ty = parse_type_expr(annotation, &alias_names, &mut vars, &mut next_var)
        .ok_or_else(|| format!("invalid string capture type annotation `{annotation}`"))?;
    Ok((name.to_string(), expand_type_aliases(&ty, aliases)))
}

fn string_capture_annotation_has_type_syntax(annotation: &str) -> bool {
    annotation.chars().all(|ch| {
        ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '[' | ']' | ',' | ' ' | '\t')
    })
}
