use std::collections::BTreeMap;

use super::{EbnfCompileError, EbnfCompileResult, ShapePattern, SyntaxExprKind, SyntaxExprOutput};

pub(super) fn reject_runtime_shape_call(
    expression: &SyntaxExprOutput,
    shapes: &BTreeMap<String, ShapePattern>,
) -> EbnfCompileResult<()> {
    if expression.kind != SyntaxExprKind::Call || expression.remote.is_some() {
        return Ok(());
    }
    let Some(callee) = expression.children.first() else {
        return Ok(());
    };
    if callee.kind != SyntaxExprKind::Var {
        return Ok(());
    }
    let Some(name) = callee.text.as_deref() else {
        return Ok(());
    };
    if shapes.contains_key(name) {
        return Err(EbnfCompileError::Serialize(format!(
            "shape `{name}` is compile-time pattern-only and cannot be called as a runtime value"
        )));
    }
    Ok(())
}
