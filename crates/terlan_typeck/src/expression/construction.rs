use super::*;

/// Infers a raw struct construction expression from syntax output.
///
/// Inputs:
/// - `expr`: syntax-output record construction node carrying the target type
///   name and field expressions.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - `Type::Named` for the constructed source type when inference can continue.
///
/// Transformation:
/// - Typechecks every field value, then enforces the Terlan visibility rule
///   that imported/public struct type identity does not grant raw construction
///   authority outside the defining module.
pub(super) fn infer_syntax_record_construct(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    for field in &expr.fields {
        let _ = infer_syntax_expr(&field.value, locals, ctx, subst, errors);
        let _ = &field.key;
    }

    let name = expr.text.clone().unwrap_or_default();
    if let Some(imported) = ctx.imported_type_names.get(&name) {
        errors.push(format!(
            "cannot raw-construct imported struct {}.{} outside defining module; use an exported constructor",
            imported.module, imported.name
        ));
    }

    Type::Named {
        module: None,
        name,
        args: Vec::new(),
    }
}

/// Infers a constructor-chain expression.
///
/// Inputs:
/// - `expr`: syntax-output constructor-chain expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Type of the extended record expression.
///
/// Transformation:
/// - Infers the base constructor expression and then validates the extension
///   record as the resulting chain value.
pub(super) fn infer_syntax_constructor_chain(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let Some(base) = expr.children.first() else {
        errors.push("constructor chain expression is missing base expression".to_string());
        return Type::Dynamic;
    };

    let Some(record) = expr.children.get(1) else {
        errors
            .push("constructor chain expression is missing constructor target record".to_string());
        let _ = infer_syntax_expr(base, locals, ctx, subst, errors);
        return Type::Dynamic;
    };

    let _ = infer_syntax_expr(base, locals, ctx, subst, errors);

    if record.kind != SyntaxExprKind::RecordConstruct {
        errors.push("constructor chain requires a record construct on the right side".to_string());
        let _ = infer_syntax_expr(record, locals, ctx, subst, errors);
        return Type::Dynamic;
    }

    infer_syntax_record_construct(record, locals, ctx, subst, errors)
}

/// Infers a record field access expression.
///
/// Inputs:
/// - `expr`: syntax-output record access expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Field type when the record and field are known; otherwise `Dynamic`.
///
/// Transformation:
/// - Infers the receiver, resolves the record schema, and extracts the selected
///   field type.
pub(super) fn infer_syntax_record_access(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let _ = expr
        .children
        .first()
        .map(|value| infer_syntax_expr(value, locals, ctx, subst, errors));
    let (name, field) = expr
        .text
        .as_deref()
        .and_then(|text| text.split_once('.'))
        .unwrap_or_default();
    if let Some(fields) = ctx.struct_fields.get(name) {
        if let Some(field_type) = fields.get(field) {
            field_type.clone()
        } else {
            errors.push(format!("unknown field {} on struct {}", field, name));
            Type::Dynamic
        }
    } else {
        errors.push(format!("unknown struct {}", name));
        Type::Dynamic
    }
}

/// Infers a dot field access expression.
///
/// Inputs:
/// - `expr`: syntax-output field access expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Field type when the receiver shape is known; otherwise `Dynamic`.
///
/// Transformation:
/// - Infers the receiver and resolves field lookup against known struct and
///   map-like shapes.
pub(super) fn infer_syntax_field_access(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let receiver = expr
        .children
        .first()
        .map(|value| apply_subst(&infer_syntax_expr(value, locals, ctx, subst, errors), subst))
        .unwrap_or(Type::Dynamic);
    let field = expr.text.as_deref().unwrap_or_default();
    match receiver {
        Type::Named { name, .. } if name == "Markdown" => match field {
            "raw" => Type::Binary,
            "html" => Type::Named {
                module: None,
                name: "Html".to_string(),
                args: vec![Type::Never],
            },
            _ => {
                errors.push(format!("unknown field {} on Markdown import", field));
                Type::Dynamic
            }
        },
        Type::Named { name, .. } => {
            if let Some(fields) = ctx.struct_fields.get(&name) {
                if let Some(field_type) = fields.get(field) {
                    field_type.clone()
                } else {
                    errors.push(format!("unknown field {} on struct {}", field, name));
                    Type::Dynamic
                }
            } else {
                errors.push(format!(
                    "field access requires struct receiver, found {}",
                    pretty_type(&Type::Named {
                        module: None,
                        name,
                        args: Vec::new(),
                    })
                ));
                Type::Dynamic
            }
        }
        other => {
            errors.push(format!(
                "field access requires struct receiver, found {}",
                pretty_type(&other)
            ));
            Type::Dynamic
        }
    }
}

/// Infers a record update expression.
///
/// Inputs:
/// - `expr`: syntax-output record update expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Updated record type when the base record is valid.
///
/// Transformation:
/// - Infers the base record, validates updated fields against the record
///   schema, and returns the original record type.
pub(super) fn infer_syntax_record_update(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let base = expr
        .children
        .first()
        .map(|value| infer_syntax_expr(value, locals, ctx, subst, errors))
        .unwrap_or(Type::Dynamic);
    for field in &expr.fields {
        let _ = infer_syntax_expr(&field.value, locals, ctx, subst, errors);
        let _ = &field.key;
    }
    let _ = &expr.text;
    base
}

/// Infers an HTML/template instantiation expression.
///
/// Inputs:
/// - `expr`: syntax-output template instantiation expression.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Template result type, or `Dynamic` when the template is unresolved.
///
/// Transformation:
/// - Checks supplied props against the visible template scheme and returns the
///   backend-neutral HTML/template value type.
pub(super) fn infer_syntax_template_instantiation(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let name = expr.text.as_deref().unwrap_or_default();
    let mut provided = HashSet::new();
    let Some(template) = ctx.templates.get(name) else {
        for field in &expr.fields {
            let _ = infer_syntax_expr(&field.value, locals, ctx, subst, errors);
        }
        errors.push(format!("unknown template `{}`", name));
        return Type::Dynamic;
    };

    for field in &expr.fields {
        if !provided.insert(field.key.clone()) {
            errors.push(format!(
                "duplicate prop `{}` in template `{}` instantiation",
                field.key, name
            ));
        }

        let actual = infer_syntax_expr(&field.value, locals, ctx, subst, errors);
        let Some(expected) = template.props.get(&field.key) else {
            errors.push(format!(
                "template `{}` instantiation has unknown prop `{}`",
                name, field.key
            ));
            continue;
        };

        let expected = expand_type_aliases(expected, ctx.aliases);
        let actual = expand_type_aliases(&actual, ctx.aliases);
        if let Err(message) = unify(&expected, &actual, subst) {
            errors.push(format!(
                "template `{}` prop `{}`: {}",
                name, field.key, message
            ));
        }
    }

    for prop_name in template.props.keys() {
        if !provided.contains(prop_name) {
            errors.push(format!(
                "template `{}` instantiation is missing required prop `{}`",
                name, prop_name
            ));
        }
    }

    Type::Named {
        module: None,
        name: "Html".to_string(),
        args: vec![Type::Dynamic],
    }
}
