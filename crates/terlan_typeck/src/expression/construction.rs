use super::*;
use terlan_hir::FunctionSignature;

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
/// - Typechecks every field value against known local struct field types, uses
///   field type context for module-member function values, and enforces the
///   Terlan visibility rule that imported/public struct type identity does not
///   grant raw construction authority outside the defining module.
pub(super) fn infer_syntax_record_construct(
    expr: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Type {
    let name = expr.text.clone().unwrap_or_default();
    if let Some(known_fields) = ctx.struct_fields.get(&name) {
        for field in &expr.fields {
            let (lookup_field, requested_private) = split_private_field_spelling(&field.key);
            let Some(expected_field_type) = known_fields.get(lookup_field) else {
                errors.push(format!("unknown field {} on struct {}", lookup_field, name));
                let _ = infer_syntax_expr(&field.value, locals, ctx, subst, errors);
                continue;
            };

            if let Some(message) = struct_field_visibility_error(
                &name,
                lookup_field,
                requested_private,
                ctx.struct_field_visibility,
                ctx.imported_type_names,
            ) {
                errors.push(message);
            }

            check_record_construct_field_value(
                &name,
                lookup_field,
                expected_field_type,
                &field.value,
                locals,
                ctx,
                subst,
                errors,
            );
        }
    } else {
        for field in &expr.fields {
            let _ = infer_syntax_expr(&field.value, locals, ctx, subst, errors);
        }
    }

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

/// Checks one struct construction field against the declared field type.
///
/// Inputs:
/// - `struct_name` and `field_name`: diagnostic identity for the field.
/// - `expected`: declared field type from local struct metadata.
/// - `value`: syntax-output expression assigned to the field.
/// - `locals`, `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - No direct return value.
///
/// Transformation:
/// - Infers the value with field-type context where supported, falls back to
///   ordinary inference, expands aliases, and unifies expected and actual field
///   types so raw construction cannot bypass struct field contracts.
fn check_record_construct_field_value(
    struct_name: &str,
    field_name: &str,
    expected: &Type,
    value: &SyntaxExprOutput,
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) {
    let expected_expanded = expand_type_aliases(expected, ctx.aliases);
    let actual = infer_imported_module_member_function_value_with_expected(
        value,
        &expected_expanded,
        ctx,
        subst,
        errors,
    )
    .unwrap_or_else(|| infer_syntax_expr(value, locals, ctx, subst, errors));
    let actual_expanded = expand_type_aliases(&actual, ctx.aliases);
    if let Err(message) = unify(&expected_expanded, &actual_expanded, subst) {
        errors.push(format!(
            "field `{}` on struct `{}` {}",
            field_name, struct_name, message
        ));
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
    if let Some(module_member) = infer_imported_module_member_function_value(expr, ctx, errors) {
        return module_member;
    }

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
            let (lookup_field, requested_private) = split_private_field_spelling(field);
            if let Some(fields) = ctx.struct_fields.get(&name) {
                if let Some(field_type) = fields.get(lookup_field) {
                    if let Some(message) = struct_field_visibility_error(
                        &name,
                        lookup_field,
                        requested_private,
                        ctx.struct_field_visibility,
                        ctx.imported_type_names,
                    ) {
                        errors.push(message);
                    }
                    field_type.clone()
                } else {
                    errors.push(format!("unknown field {} on struct {}", lookup_field, name));
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

/// Infers `Module.function` as an imported function value.
///
/// Inputs:
/// - `expr`: syntax-output field access that may have an imported module alias
///   receiver and a function member name.
/// - `ctx`: expression inference context containing module aliases and loaded
///   imported interfaces.
/// - `errors`: mutable diagnostic sink for malformed module-member references.
///
/// Output:
/// - `Some(Type::Function)` when the receiver is an imported module alias and
///   the module exposes exactly one public function with the requested name.
/// - `Some(Type::Dynamic)` when the receiver is an imported module alias but
///   the member is missing, ambiguous, or unparsable.
/// - `None` when the expression is ordinary value field access.
///
/// Transformation:
/// - Resolves the receiver name through module imports, reads the provider
///   interface, parses the public function signature, and converts that
///   signature into a first-class function value type without changing parser
///   output.
fn infer_imported_module_member_function_value(
    expr: &SyntaxExprOutput,
    ctx: &ExprInferContext,
    errors: &mut Vec<String>,
) -> Option<Type> {
    if !matches!(expr.kind, SyntaxExprKind::FieldAccess) || expr.children.len() != 1 {
        return None;
    }

    let module_alias = syntax_field_access_receiver_name(expr)?;
    let resolved_module = ctx.module_aliases.get(module_alias)?;
    let member = expr.text.as_deref().unwrap_or_default();
    let Some(interface) = ctx.interface_map.get(resolved_module) else {
        errors.push(format!(
            "cannot find interface for imported module `{}`",
            resolved_module
        ));
        return Some(Type::Dynamic);
    };

    let signatures = unique_public_member_signatures(interface, member);
    let [signature] = signatures.as_slice() else {
        if signatures.is_empty() {
            errors.push(format!(
                "module `{}` has no exported function `{}`",
                resolved_module, member
            ));
        } else {
            errors.push(format!(
                "module-member function value `{}.{}` is ambiguous without an expected function type",
                module_alias, member
            ));
        }
        return Some(Type::Dynamic);
    };

    let Some(scheme) = parse_interface_signature(signature, interface, ctx.aliases) else {
        errors.push(format!(
            "cannot parse imported function signature {}.{} / {}",
            resolved_module,
            member,
            signature.params.len()
        ));
        return Some(Type::Dynamic);
    };
    let instantiated = instantiate_function_scheme(&scheme);
    Some(Type::Function {
        params: instantiated.params,
        ret: Box::new(instantiated.ret),
    })
}

/// Infers `Module.function` against an expected function value type.
///
/// Inputs:
/// - `expr`: syntax-output field access that may be an imported module-member
///   function value.
/// - `expected`: expected contextual type, normally a call parameter type.
/// - `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - `Some(Type::Function)` when the member reference resolves to exactly one
///   public provider signature compatible with `expected`.
/// - `Some(Type::Dynamic)` when the receiver is an imported module alias but
///   the member is missing, unparsable, or ambiguous in the expected context.
/// - `None` when the expression is ordinary value field access or the expected
///   type is not a function value.
///
/// Transformation:
/// - Resolves the module alias, parses public member signatures, instantiates
///   each candidate, and uses ordinary type unification to select the candidate
///   that matches the contextual function type.
pub(super) fn infer_imported_module_member_function_value_with_expected(
    expr: &SyntaxExprOutput,
    expected: &Type,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    if !matches!(expected, Type::Function { .. }) {
        return None;
    }
    if !matches!(expr.kind, SyntaxExprKind::FieldAccess) || expr.children.len() != 1 {
        return None;
    }

    let module_alias = syntax_field_access_receiver_name(expr)?;
    let resolved_module = ctx.module_aliases.get(module_alias)?;
    let member = expr.text.as_deref().unwrap_or_default();
    let Some(interface) = ctx.interface_map.get(resolved_module) else {
        errors.push(format!(
            "cannot find interface for imported module `{}`",
            resolved_module
        ));
        return Some(Type::Dynamic);
    };

    let signatures = unique_public_member_signatures(interface, member);
    if signatures.is_empty() {
        errors.push(format!(
            "module `{}` has no exported function `{}`",
            resolved_module, member
        ));
        return Some(Type::Dynamic);
    }

    let mut selected = Vec::new();
    for signature in signatures {
        let Some(scheme) = parse_interface_signature(signature, interface, ctx.aliases) else {
            errors.push(format!(
                "cannot parse imported function signature {}.{} / {}",
                resolved_module,
                member,
                signature.params.len()
            ));
            return Some(Type::Dynamic);
        };
        let instantiated = instantiate_function_scheme(&scheme);
        let candidate = Type::Function {
            params: instantiated.params,
            ret: Box::new(instantiated.ret),
        };
        let mut trial_subst = subst.clone();
        let matches_expected = unify(&candidate, expected, &mut trial_subst).is_ok() || {
            let mut expanded_subst = subst.clone();
            let candidate_expanded = expand_type_aliases(&candidate, ctx.aliases);
            let expected_expanded = expand_type_aliases(expected, ctx.aliases);
            unify(&candidate_expanded, &expected_expanded, &mut expanded_subst)
                .map(|_| {
                    trial_subst = expanded_subst;
                })
                .is_ok()
        };
        if matches_expected {
            selected.push((candidate, trial_subst));
        }
    }

    match selected.len() {
        1 => {
            let (candidate, trial_subst) = selected.pop().expect("selected length was checked");
            *subst = trial_subst;
            Some(candidate)
        }
        0 => {
            errors.push(format!(
                "module-member function value `{}.{}` does not match expected {}",
                module_alias,
                member,
                pretty_type(expected)
            ));
            Some(Type::Dynamic)
        }
        _ => {
            errors.push(format!(
                "module-member function value `{}.{}` is ambiguous for expected {}",
                module_alias,
                member,
                pretty_type(expected)
            ));
            Some(Type::Dynamic)
        }
    }
}

/// Returns the receiver name from a field access expression.
///
/// Inputs:
/// - `expr`: syntax-output field access node.
///
/// Output:
/// - Receiver identifier text for variable-like receivers.
/// - `None` when the receiver is not a name expression.
///
/// Transformation:
/// - Accepts both `Var` and `Atom` syntax-output nodes because uppercase
///   module aliases are currently represented as name-like atom nodes.
fn syntax_field_access_receiver_name(expr: &SyntaxExprOutput) -> Option<&str> {
    let receiver = expr.children.first()?;
    match receiver.kind {
        SyntaxExprKind::Var | SyntaxExprKind::Atom => receiver.text.as_deref(),
        _ => None,
    }
}

/// Returns public interface signatures for one member reference.
///
/// Inputs:
/// - `interface`: provider module interface.
/// - `member`: requested function/member name.
///
/// Output:
/// - Public overload signatures when present; otherwise public compatibility
///   signatures for the same name.
///
/// Transformation:
/// - Mirrors imported call precedence by preferring overload metadata over the
///   compatibility function map, while leaving ambiguity resolution to the
///   caller.
fn unique_public_member_signatures<'a>(
    interface: &'a ModuleInterface,
    member: &str,
) -> Vec<&'a FunctionSignature> {
    let overloads = interface
        .function_overloads
        .iter()
        .filter(|((name, _), _)| name == member)
        .flat_map(|(_, signatures)| signatures.iter())
        .filter(|signature| signature.public)
        .collect::<Vec<_>>();
    if !overloads.is_empty() {
        return overloads;
    }

    interface
        .functions
        .iter()
        .filter(|((name, _), signature)| name == member && signature.public)
        .map(|(_, signature)| signature)
        .collect()
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
        if let Type::Named { name, .. } = &base {
            let (lookup_field, requested_private) = split_private_field_spelling(&field.key);
            if let Some(fields) = ctx.struct_fields.get(name) {
                if fields.contains_key(lookup_field) {
                    if let Some(message) = struct_field_visibility_error(
                        name,
                        lookup_field,
                        requested_private,
                        ctx.struct_field_visibility,
                        ctx.imported_type_names,
                    ) {
                        errors.push(message);
                    }
                } else {
                    errors.push(format!("unknown field {} on struct {}", lookup_field, name));
                }
            }
        }
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
        if ctx.struct_fields.contains_key(name) || ctx.imported_type_names.contains_key(name) {
            let mut record_expr = expr.clone();
            record_expr.kind = SyntaxExprKind::RecordConstruct;
            return infer_syntax_record_construct(&record_expr, locals, ctx, subst, errors);
        }
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
        let Some(prop) = template.props.get(&field.key) else {
            errors.push(format!(
                "template `{}` instantiation has unknown prop `{}`",
                name, field.key
            ));
            continue;
        };

        let expected = expand_type_aliases(&prop.ty, ctx.aliases);
        let actual = expand_type_aliases(&actual, ctx.aliases);
        if let Err(message) = unify(&expected, &actual, subst) {
            errors.push(format!(
                "template `{}` prop `{}`: {}",
                name, field.key, message
            ));
        }
    }

    for (prop_name, prop) in &template.props {
        if provided.contains(prop_name) {
            continue;
        }

        let Some(default) = &prop.default else {
            errors.push(format!(
                "template `{}` instantiation is missing required prop `{}`",
                name, prop_name
            ));
            continue;
        };

        let actual = infer_syntax_expr(default, locals, ctx, subst, errors);
        let expected = expand_type_aliases(&prop.ty, ctx.aliases);
        let actual = expand_type_aliases(&actual, ctx.aliases);
        if let Err(message) = unify(&expected, &actual, subst) {
            errors.push(format!(
                "template `{}` default prop `{}`: {}",
                name, prop_name, message
            ));
        }
    }

    Type::Named {
        module: None,
        name: "Html".to_string(),
        args: vec![Type::Dynamic],
    }
}
