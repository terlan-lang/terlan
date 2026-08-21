use super::*;

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
        CoreExpr::Lam { body, .. } => visit(body),
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

pub(super) fn list_element(ty: &CoreType) -> Option<&CoreType> {
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

pub(super) fn function_return_type(
    functions: &FunctionTypes,
    module: &str,
    function: &str,
    arity: usize,
) -> Option<CoreType> {
    function_signature(functions, module, function, arity).map(|signature| signature.result.clone())
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
