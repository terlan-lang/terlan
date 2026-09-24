use super::*;

/// A receiver remains a lexical place until mutation is functionalized. Folding
/// a bottom read here would lose the binding before a callback is instantiated.
pub(super) fn specialize_receiver(
    expr: &mut CoreExpr,
    variables: &HashMap<String, CoreType>,
    functions: &FunctionTypes,
    module: &str,
) -> Option<CoreType> {
    match expr {
        CoreExpr::Var(name) => variables.get(name).cloned(),
        _ => specialize_expr(expr, variables, functions, module),
    }
}

pub(super) fn specialize_intrinsic_arguments(
    call: &mut CoreIntrinsicCall,
    variables: &HashMap<String, CoreType>,
    functions: &FunctionTypes,
    module: &str,
) -> Option<Vec<Option<CoreType>>> {
    let mutating_list = matches!(
        call.id,
        CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::ListPush | CorePrimitiveIntrinsic::ListClear
        )
    );
    let mut types = call
        .args
        .iter_mut()
        .enumerate()
        .map(|(index, argument)| {
            if index == 0 && mutating_list {
                specialize_receiver(argument, variables, functions, module)
            } else {
                specialize_expr(argument, variables, functions, module)
            }
        })
        .collect::<Vec<_>>();
    if call.id == CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListPush) {
        if let Some(receiver_type) = types.first().cloned().flatten() {
            let refined = refine_mutating_receiver(
                &mut call.args[0],
                Some(receiver_type),
                "push",
                &types[1..],
            )?;
            types[0] = Some(refined);
        }
    }
    Some(types)
}

/// A push changes the new collection value, never the earlier binding's type.
pub(super) fn refine_mutating_receiver(
    receiver: &mut CoreExpr,
    receiver_type: Option<CoreType>,
    method: &str,
    argument_types: &[Option<CoreType>],
) -> Option<CoreType> {
    let ty = receiver_type?;
    if method == "push" && super::super::empty_list_values::is_bottom_list(&ty) {
        let element = argument_types.first()?.as_ref()?.clone();
        let refined = CoreType::List(Box::new(element));
        super::super::empty_list_values::coerce(receiver, &ty, &refined);
        Some(refined)
    } else {
        Some(ty)
    }
}

/// Unknown rebinding types invalidate earlier witnesses until instantiation.
pub(super) fn replace_binding_type(
    pattern: &CorePattern,
    ty: Option<&CoreType>,
    variables: &mut HashMap<String, CoreType>,
) {
    for name in super::super::expression::free_variable_analysis::pattern_bound_names(pattern) {
        variables.remove(&name);
    }
    if let Some(ty) = ty {
        bind_pattern(pattern, ty, variables);
    }
}

/// A checked List[Never] can contain no values. Materialize its empty value at
/// each read so independent consumers can supply independent concrete schemas.
/// Only reads are replaced: the binding's producer still executes exactly once.
pub(super) fn specialize_variable(
    expr: &mut CoreExpr,
    variables: &HashMap<String, CoreType>,
) -> Option<CoreType> {
    let CoreExpr::Var(name) = expr else {
        return None;
    };
    let ty = variables.get(name)?.clone();
    if super::super::empty_list_values::is_bottom_list(&ty) {
        *expr = CoreExpr::Cast {
            expr: Box::new(CoreExpr::List(Vec::new())),
            target_type: ty.clone(),
        };
    }
    Some(ty)
}

/// Empty constructors retain uninhabited slots when no consumer supplies a type.
pub(super) fn normalize_empty_collection_constructor(call: &mut CoreIntrinsicCall) {
    if call.id == CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListNew)
        && (is_dynamic_type(&call.return_type)
            || list_element(&call.return_type).is_some_and(is_dynamic_type))
    {
        call.return_type = CoreType::List(Box::new(CoreType::Never));
    }
    if call.id == CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapNew) {
        match &mut call.return_type {
            CoreType::Apply { constructor, args }
                if matches!(constructor.as_str(), "Map" | "std.collections.Map.Map")
                    && args.len() == 2 =>
            {
                for slot in args {
                    if is_dynamic_type(slot) {
                        *slot = CoreType::Never;
                    }
                }
            }
            CoreType::Named(name) if name == "Map" => {
                call.return_type = CoreType::Apply {
                    constructor: "Map".to_string(),
                    args: vec![CoreType::Never, CoreType::Never],
                };
            }
            _ => {}
        }
    }
}

/// Persist collection types inferred from callable signatures. The final schema
/// inventory cannot reconstruct a list element's type from a call name alone.
pub(super) fn preserve_inferred_list_type(
    expr: &mut CoreExpr,
    ty: &CoreType,
    functions: &FunctionTypes,
    module: &str,
) {
    if super::super::collections::managed_collection_layouts([ty]).is_ok() {
        specialize_expected_collection_new(expr, ty, functions, module);
    }
}

/// Visits every checked homogeneous element and merges its type witnesses.
/// Empty nested lists contribute Never, which concrete siblings can refine.
pub(super) fn specialize_elements(
    items: &mut [CoreExpr],
    variables: &HashMap<String, CoreType>,
    functions: &FunctionTypes,
    module: &str,
) -> Option<CoreType> {
    let mut witness = items.is_empty().then_some(CoreType::Never);
    let mut incompatible = false;
    for item in items {
        let inferred = specialize_expr(item, variables, functions, module);
        if let Some(inferred) = inferred.filter(|_| !incompatible) {
            witness = match witness {
                Some(prior) => super::super::structured_case::merge_control_types(prior, inferred),
                None => Some(inferred),
            };
            incompatible = witness.is_none();
        }
    }
    witness
}

/// Intrinsics retain their result type but no parameter signature for metadata
/// assembly. Preserve inferred list operands through the normal checked-argument
/// path so lists of factory results retain their runtime collection descriptor.
pub(super) fn preserve_list_operands(
    args: &mut [CoreExpr],
    types: &[Option<CoreType>],
    functions: &FunctionTypes,
    module: &str,
) {
    for (argument, inferred) in args.iter_mut().zip(types) {
        if let Some(ty @ CoreType::List(_)) = inferred {
            if matches!(argument, CoreExpr::List(_))
                && super::super::collections::managed_collection_layouts([ty]).is_ok()
            {
                specialize_expected_collection_new(argument, ty, functions, module);
            }
        }
    }
}

pub(super) fn visit_children(
    expr: &mut CoreExpr,
    variables: &HashMap<String, CoreType>,
    functions: &FunctionTypes,
    module: &str,
) {
    let mut visit = |expr: &mut CoreExpr| {
        specialize_expr(expr, variables, functions, module);
    };
    match expr {
        CoreExpr::Tuple(items) | CoreExpr::FixedArray(items) => {
            items.iter_mut().for_each(&mut visit)
        }
        CoreExpr::Index { base, index } => {
            visit(base);
            visit(index);
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            visit(expr);
            generators
                .iter_mut()
                .for_each(|generator| visit(&mut generator.source));
            guards.iter_mut().for_each(&mut visit);
        }
        CoreExpr::Map(fields) => fields.iter_mut().for_each(|field| visit(&mut field.value)),
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            fields.iter_mut().for_each(|field| visit(&mut field.value));
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            visit(base);
            fields.iter_mut().for_each(|field| visit(&mut field.value));
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => visit(base),
        CoreExpr::ConstructorChain { args, record, .. } => {
            args.iter_mut().for_each(&mut visit);
            visit(record);
        }
        CoreExpr::ConstructorCall { args, .. } => args.iter_mut().for_each(&mut visit),
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            visit(receiver);
            args.iter_mut().for_each(&mut visit);
        }
        CoreExpr::FunctionCall { callee, args } => {
            visit(callee);
            args.iter_mut().for_each(&mut visit);
        }
        CoreExpr::SqlQuery { parameters, .. } => parameters.iter_mut().for_each(&mut visit),
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            visit(body);
            for clause in of_clauses.iter_mut().chain(catch_clauses) {
                if let Some(guard) = clause.guard.as_mut() {
                    visit(guard);
                }
                visit(&mut clause.body);
            }
            if let Some(after) = after_clause {
                visit(&mut after.trigger);
                visit(&mut after.body);
            }
        }
        CoreExpr::Lam {
            params,
            parameter_types,
            body,
        } => {
            let locals = super::super::generic_specialization::lambda_type_scope(
                params,
                parameter_types,
                variables,
            );
            specialize_expr(body, &locals, functions, module);
        }
        _ => {}
    }
}

pub(super) fn bind_pattern(
    pattern: &CorePattern,
    ty: &CoreType,
    variables: &mut HashMap<String, CoreType>,
) {
    match pattern {
        CorePattern::Var(name) => {
            variables.insert(name.clone(), ty.clone());
        }
        CorePattern::Alias { alias, pattern } => {
            variables.insert(alias.clone(), ty.clone());
            bind_pattern(pattern, ty, variables);
        }
        CorePattern::List(_) | CorePattern::ListCons { .. } => {
            // Share lexical list binding with generic and receiver resolution;
            // dropping these bindings leaves element methods as raw calls.
            super::super::generic_specialization::bind_pattern_types(pattern, ty, variables);
        }
        CorePattern::Constructor { .. } | CorePattern::Tuple(_)
            if matches!(ty, CoreType::Apply { constructor, args }
                if constructor == "std.core.Result.Result" && args.len() == 2) =>
        {
            // Intrinsic results can introduce the canonical alias after the
            // expansion pass. Reuse checked Result payload binding before the
            // next pass materializes its structural union.
            super::super::generic_specialization::bind_pattern_types(pattern, ty, variables);
        }
        CorePattern::Constructor { name, args, .. } if name == "Some" => {
            if let (Some(element), [pattern]) = (option_element(ty), args.as_slice()) {
                bind_pattern(pattern, element, variables);
            } else if let Some(elements) = tagged_union_tuple(ty, "some") {
                for (pattern, element) in args.iter().zip(elements.iter().skip(1)) {
                    bind_pattern(pattern, tuple_element_type(element), variables);
                }
            }
        }
        CorePattern::Constructor { name, args, .. } => {
            let tag = if name == "Err" {
                "error".to_string()
            } else {
                name.to_lowercase()
            };
            let Some(elements) = tagged_union_tuple(ty, &tag) else {
                return;
            };
            for (pattern, element) in args.iter().zip(elements.iter().skip(1)) {
                bind_pattern(pattern, tuple_element_type(element), variables);
            }
        }
        CorePattern::Tuple(patterns) => {
            let tuple = match ty {
                CoreType::Tuple(elements) => Some(elements.as_slice()),
                CoreType::Union(_) => patterns
                    .first()
                    .and_then(pattern_atom)
                    .and_then(|tag| tagged_union_tuple(ty, tag)),
                _ => None,
            };
            let Some(elements) = tuple else {
                return;
            };
            for (pattern, element) in patterns.iter().zip(elements) {
                bind_pattern(pattern, tuple_element_type(element), variables);
            }
        }
        CorePattern::Map(patterns) => {
            let CoreType::Map(fields) = ty else {
                return;
            };
            for pattern in patterns {
                if let Some(field) = fields.iter().find(|field| field.key == pattern.key) {
                    bind_pattern(&pattern.value, &field.value, variables);
                }
            }
        }
        CorePattern::BinaryLayout { fields, .. } => {
            for field in fields {
                if field.name == "_" {
                    continue;
                }
                let field_type = match field.descriptor {
                    crate::terlan_typeck::CoreBinaryPatternDescriptor::Bytes(_)
                    | crate::terlan_typeck::CoreBinaryPatternDescriptor::Rest => {
                        CoreType::Named("Bytes".to_string())
                    }
                    crate::terlan_typeck::CoreBinaryPatternDescriptor::Bits(_) => {
                        CoreType::Named("BitString".to_string())
                    }
                    crate::terlan_typeck::CoreBinaryPatternDescriptor::UInt(_)
                    | crate::terlan_typeck::CoreBinaryPatternDescriptor::IntBits(_)
                    | crate::terlan_typeck::CoreBinaryPatternDescriptor::Utf8
                    | crate::terlan_typeck::CoreBinaryPatternDescriptor::Utf16
                    | crate::terlan_typeck::CoreBinaryPatternDescriptor::Utf32 => CoreType::Int,
                };
                variables.insert(field.name.clone(), field_type);
            }
        }
        _ => {}
    }
}

pub(super) fn pattern_atom(pattern: &CorePattern) -> Option<&str> {
    match pattern {
        CorePattern::Atom(atom) => Some(atom),
        CorePattern::Alias { pattern, .. } => pattern_atom(pattern),
        _ => None,
    }
}

pub(super) fn tagged_union_tuple<'a>(
    ty: &'a CoreType,
    tag: &str,
) -> Option<&'a [crate::terlan_typeck::CoreTupleTypeElem]> {
    let CoreType::Union(variants) = ty else {
        return None;
    };
    variants.iter().find_map(|variant| {
        let CoreType::Tuple(elements) = variant else {
            return None;
        };
        let first = elements.first().map(tuple_element_type);
        matches!(first, Some(CoreType::AtomLiteral(atom)) if atom == tag)
            .then_some(elements.as_slice())
    })
}

pub(in crate::compiler::native_ir) fn list_element(ty: &CoreType) -> Option<&CoreType> {
    match ty {
        CoreType::List(element) => Some(element),
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("List") && args.len() == 1 =>
        {
            Some(&args[0])
        }
        _ => None,
    }
}

pub(super) fn iterator_element(ty: &CoreType) -> Option<&CoreType> {
    match ty {
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Iterator") && args.len() == 1 =>
        {
            Some(&args[0])
        }
        _ => None,
    }
}

pub(super) fn function_signature<'a>(
    functions: &'a FunctionTypes,
    module: &str,
    function: &str,
    arity: usize,
) -> Option<&'a FunctionSignature> {
    if let Some(signature) = functions
        .get(&(module.to_string(), function.to_string(), arity))
        .or_else(|| {
            function.rsplit_once('.').and_then(|(owner, name)| {
                functions.get(&(owner.to_string(), name.to_string(), arity))
            })
        })
    {
        return Some(signature);
    }
    let name = function.rsplit('.').next().unwrap_or(function);
    let mut matches = functions
        .iter()
        .filter(|((_, candidate, candidate_arity), _)| {
            candidate == name && *candidate_arity == arity
        })
        .map(|(_, signature)| signature);
    let signature = matches.next()?;
    matches.next().is_none().then_some(signature)
}

/// Substitute available argument witnesses before retaining a call's result.
/// Unresolved parameters remain symbolic context for structural constructors;
/// the monomorphizer, not this early collection pass, owns complete inference.
pub(super) fn instantiated_result_type(
    signature: &FunctionSignature,
    type_args: &[CoreType],
    argument_types: &[Option<CoreType>],
) -> Option<CoreType> {
    use super::super::generic_specialization::{contains_generic_parameter, substitute, unify};

    if !contains_generic_parameter(&signature.result, &signature.generic_params) {
        return Some(signature.result.clone());
    }
    if (!type_args.is_empty() && type_args.len() != signature.generic_params.len())
        || argument_types.len() != signature.params.len()
    {
        return None;
    }
    let mut values = signature
        .generic_params
        .iter()
        .cloned()
        .zip(type_args.iter().cloned())
        .collect();
    for (parameter, argument) in signature.params.iter().zip(argument_types) {
        if let Some(argument) = argument {
            unify(parameter, argument, &signature.generic_params, &mut values).ok()?;
        }
    }
    Some(substitute(
        &signature.result,
        &signature.generic_params,
        &values,
    ))
}

/// Retains the checked Effect result through intrinsic replacement and aliases.
/// Reuses the declaration's generic constraints instead of guessing from fields,
/// callback input types or an unrelated user-defined type named Effect.
pub(super) fn specialize_declared_effect_result(
    call: &mut CoreIntrinsicCall,
    argument_types: &[Option<CoreType>],
    functions: &FunctionTypes,
) {
    if call.id != CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmEffectRun)
        || call.args.len() != 1
        || !matches!(argument_types, [Some(_)])
    {
        return;
    }
    let Some(effect) =
        functions.get(&("std.core.Effect".to_string(), nominal_type_key("Effect"), 0))
    else {
        return;
    };
    let [value] = effect.generic_params.as_slice() else {
        return;
    };
    // Intrinsic calls do not keep the source `run` function reachable. The
    // canonical type declaration survives executable-function pruning instead.
    let signature = FunctionSignature {
        generic_params: effect.generic_params.clone(),
        params: vec![effect.result.clone()],
        result: CoreType::Named(value.clone()),
    };
    if let Some(result) = instantiated_result_type(&signature, &[], argument_types) {
        call.return_type = result;
    }
}

pub(super) fn is_bytes(ty: &CoreType) -> bool {
    matches!(
        ty,
        CoreType::Named(name)
            if matches!(
                name.rsplit('.').next(),
                Some("Bytes")
            )
    )
}

pub(super) fn is_bitstring(ty: &CoreType) -> bool {
    match ty {
        CoreType::Binary => true,
        CoreType::Named(name) => {
            matches!(name.rsplit('.').next(), Some("Binary" | "BitString"))
        }
        _ => false,
    }
}
