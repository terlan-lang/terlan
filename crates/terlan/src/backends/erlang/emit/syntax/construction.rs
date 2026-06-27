use super::*;

/// Lowers a constructor-chain expression into the current Erlang bridge shape.
///
/// Inputs:
/// - `expr`: syntax-output constructor-chain expression with base and record
///   children.
/// - `ctx`: syntax lowering context with constructor and import metadata.
/// - `env`: local lowering environment for base and field expressions.
///
/// Output:
/// - Erlang expression combining the lowered base expression and derived shape.
/// - `None` when either child is missing or cannot lower.
///
/// Transformation:
/// - Preserves both sides of `Base(args) with Derived { ... }` in an Erlang
///   `begin` expression until the formal constructor representation is moved
///   fully behind CoreIR lowering.
pub(super) fn lower_syntax_constructor_chain(
    expr: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    let base = expr.children.first()?;
    let record = expr.children.get(1)?;

    let lowered_base = lower_syntax_expr_with_env(base, ctx, env)?;
    let lowered_record = lower_syntax_constructor_extension_record(record, ctx, env)?;

    Some(ErlExpr::Raw(format!(
        "begin\n    {},\n    {}\nend",
        lowered_base.render(),
        lowered_record.render()
    )))
}

/// Lowers the derived side of constructor extension into an Erlang tuple.
///
/// Inputs:
/// - `record`: syntax-output record-construction node used after `with`.
/// - `ctx`: syntax lowering context with imports, templates, and constructor
///   metadata.
/// - `env`: local lowering environment for parameter/field-sensitive rewrites.
///
/// Output:
/// - `Some(ErlExpr::Tuple)` containing the derived shape tag followed by field
///   values in source order.
/// - `None` when the right side is not record-construction syntax or a field
///   value cannot lower.
///
/// Transformation:
/// - Treats `Base(args) with Derived { field = value }` as constructor-style
///   shape composition for the current formal Erlang path by emitting
///   `{Derived, Value}` instead of an Erlang record literal. This avoids
///   generating undeclared `#derived{}` references for constructor-extension
///   shapes that are not source-level structs.
fn lower_syntax_constructor_extension_record(
    record: &SyntaxExprOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlExpr> {
    if record.kind != SyntaxExprKind::RecordConstruct {
        return None;
    }

    let mut items = Vec::with_capacity(record.fields.len() + 1);
    items.push(ErlExpr::Atom(record.text.clone()?));
    items.extend(
        record
            .fields
            .iter()
            .map(|field| lower_syntax_expr_with_env(&field.value, ctx, env))
            .collect::<Option<Vec<_>>>()?,
    );
    Some(ErlExpr::Tuple(items))
}

/// Lowers one expression field while preserving the field's required flag.
///
/// Inputs:
/// - `field`: syntax-output expression field.
/// - `ctx`: syntax lowering context.
/// - `env`: local expression lowering environment.
///
/// Output:
/// - Erlang map/record field model with lowered value expression.
///
/// Transformation:
/// - Preserves required-field metadata for contexts that use syntax fields as
///   record/update fields rather than ordinary map construction.
pub(super) fn lower_syntax_expr_field(
    field: &SyntaxExprFieldOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlMapField> {
    Some(ErlMapField {
        key: field.key.clone(),
        value: lower_syntax_expr_with_env(&field.value, ctx, env)?,
        required: field.required,
    })
}

/// Lowers one Terlan map-construction field to an Erlang map field.
///
/// Inputs:
/// - `field`: syntax-output map field.
/// - `ctx`: syntax lowering context.
/// - `env`: local expression lowering environment.
///
/// Output:
/// - Erlang map field model with a lowered value expression.
///
/// Transformation:
/// - Emits associative construction (`=>`) because Erlang reserves `:=` for
///   matching or updating existing keys, while Terlan permits `:=` in source
///   map literals as a required-key notation.
pub(super) fn lower_syntax_map_expr_field(
    field: &SyntaxExprFieldOutput,
    ctx: &SyntaxLowerCtx,
    env: &SyntaxLowerEnv,
) -> Option<ErlMapField> {
    Some(ErlMapField {
        key: field.key.clone(),
        value: lower_syntax_expr_with_env(&field.value, ctx, env)?,
        required: false,
    })
}
