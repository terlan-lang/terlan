use std::collections::{BTreeMap, BTreeSet};

use super::super::{expr_output, pattern_output, SyntaxPatternOutput};
use super::{pattern_binding_name, ShapePattern};
use crate::terlan_syntax::{
    ebnf::{EbnfCompileError, EbnfCompileResult},
    parse_tree::{Decl, Module, ShapeDecl},
    parser::{parse_terlan_expr, parse_terlan_pattern},
};

pub(super) fn collect_shapes(module: &Module) -> EbnfCompileResult<BTreeMap<String, ShapePattern>> {
    let mut shapes = BTreeMap::new();
    for declaration in &module.declarations {
        let Decl::Shape(shape) = declaration else {
            continue;
        };
        validate_shape_head(shape)?;
        let body = parse_terlan_pattern(&shape.body).map_err(|error| {
            shape_error(
                shape,
                format!("invalid body for shape `{}`: {}", shape.name, error.message),
            )
        })?;
        let body = pattern_output(&body);
        validate_shape_body_bindings(shape, &body)?;
        let guard = shape
            .guard
            .as_deref()
            .map(parse_terlan_expr)
            .transpose()
            .map_err(|error| {
                shape_error(
                    shape,
                    format!(
                        "invalid guard for shape `{}`: {}",
                        shape.name, error.message
                    ),
                )
            })?
            .as_ref()
            .map(expr_output);
        if shapes
            .insert(
                shape.name.clone(),
                ShapePattern {
                    params: shape.params.clone(),
                    body,
                    guard,
                },
            )
            .is_some()
        {
            return Err(shape_error(
                shape,
                format!("duplicate shape declaration `{}`", shape.name),
            ));
        }
    }
    Ok(shapes)
}

pub(super) fn validate_expanded_shape_bindings(
    shape_name: &str,
    pattern: &SyntaxPatternOutput,
) -> EbnfCompileResult<()> {
    let mut seen = BTreeSet::new();
    if let Some(binding) = duplicate_pattern_binding(pattern, &mut seen) {
        return Err(EbnfCompileError::Serialize(format!(
            "shape `{shape_name}` expansion binds `{binding}` more than once; overlapping shape arguments are ambiguous"
        )));
    }
    Ok(())
}

fn validate_shape_head(shape: &ShapeDecl) -> EbnfCompileResult<()> {
    let mut params = BTreeSet::new();
    for param in &shape.params {
        let valid = param
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase() || ch == '_')
            && param
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
        if !valid {
            return Err(shape_error(
                shape,
                format!("shape `{}` has invalid parameter `{param}`", shape.name),
            ));
        }
        if !params.insert(param.as_str()) {
            return Err(shape_error(
                shape,
                format!("shape `{}` has duplicate parameter `{param}`", shape.name),
            ));
        }
    }
    Ok(())
}

fn validate_shape_body_bindings(
    shape: &ShapeDecl,
    body: &SyntaxPatternOutput,
) -> EbnfCompileResult<()> {
    let mut seen = BTreeSet::new();
    if let Some(binding) = duplicate_pattern_binding(body, &mut seen) {
        return Err(shape_error(
            shape,
            format!(
                "shape `{}` binds `{binding}` more than once; duplicate shape bindings are ambiguous",
                shape.name
            ),
        ));
    }
    Ok(())
}

pub(super) fn duplicate_pattern_binding(
    pattern: &SyntaxPatternOutput,
    seen: &mut BTreeSet<String>,
) -> Option<String> {
    if pattern.kind == super::SyntaxPatternKind::BinaryLayout {
        return super::binary_layout::duplicate_capture_binding(pattern, seen);
    }
    if let Some(binding) = pattern_binding_name(pattern) {
        if !seen.insert(binding.to_string()) {
            return Some(binding.to_string());
        }
    }
    for child in &pattern.children {
        if let Some(binding) = duplicate_pattern_binding(child, seen) {
            return Some(binding);
        }
    }
    for field in &pattern.fields {
        if let Some(binding) = duplicate_pattern_binding(&field.value, seen) {
            return Some(binding);
        }
    }
    None
}

fn shape_error(shape: &ShapeDecl, message: String) -> EbnfCompileError {
    EbnfCompileError::Parse(message, shape.span)
}
