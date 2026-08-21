//! Restores concrete generic collection result types before NativeIR lowering.

use std::collections::HashMap;

use crate::terlan_typeck::{
    core_primitive_intrinsic_return_type, core_type_from_text, CoreEffectSet, CoreExpr,
    CoreIntrinsicCall, CoreIntrinsicId, CoreLetBinding, CoreModule, CorePattern,
    CorePrimitiveIntrinsic, CoreType,
};

#[path = "collection_intrinsic_specialization/expected_constructors.rs"]
mod expected_constructors;
pub(super) use expected_constructors::annotate_expected_structural_constructors;
mod comprehension;
use comprehension::specialize_comprehension;
mod expected_new;
use expected_new::{specialize_collection_new_bindings, specialize_expected_collection_new};
pub(super) mod receiver_intrinsics;
use receiver_intrinsics::*;
mod type_helpers;
use type_helpers::*;

#[derive(Clone)]
pub(super) struct FunctionSignature {
    pub(super) params: Vec<CoreType>,
    pub(super) result: CoreType,
}

pub(super) type FunctionTypes = HashMap<(String, String, usize), FunctionSignature>;

/// Attaches each declared result type to structural constructors before
/// transparent aliases erase their nominal constructor identity.
pub(super) fn annotate_function_result_constructors(core: &mut CoreModule) {
    for function in &mut core.functions {
        let Some(expected) = function.core_return_type.as_ref() else {
            continue;
        };
        for clause in &mut function.clauses {
            if let Some(body) = clause.body.core_expr.as_mut() {
                annotate_expected_structural_constructors(body, expected);
            }
        }
    }
}

pub(super) fn specialize_collection_intrinsic_results(cores: &mut [CoreModule]) {
    let mut function_types = cores
        .iter()
        .flat_map(|core| {
            core.functions.iter().filter_map(|function| {
                let result = function
                    .core_return_type
                    .clone()
                    .or_else(|| core_type_from_text(&function.return_type))?;
                let params = function
                    .params
                    .iter()
                    .map(|parameter| {
                        parameter
                            .core_ty
                            .clone()
                            .or_else(|| core_type_from_text(&parameter.ty))
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some((
                    (core.module.clone(), function.name.clone(), function.arity),
                    FunctionSignature { params, result },
                ))
            })
        })
        .collect::<FunctionTypes>();
    for core in cores.iter() {
        for declaration in &core.types {
            if let Some(body) = declaration.core_body.clone() {
                function_types.insert(
                    (core.module.clone(), nominal_type_key(&declaration.name), 0),
                    FunctionSignature {
                        params: Vec::new(),
                        result: body,
                    },
                );
            }
        }
    }
    for core in cores {
        for function in &mut core.functions {
            let parameter_types = function
                .params
                .iter()
                .map(|parameter| parameter.core_ty.clone())
                .collect::<Vec<_>>();
            let parameter_variables = function
                .params
                .iter()
                .filter_map(|parameter| {
                    parameter
                        .core_ty
                        .clone()
                        .map(|ty| (parameter.name.clone(), ty))
                })
                .collect::<HashMap<_, _>>();
            for clause in &mut function.clauses {
                let mut variables = parameter_variables.clone();
                for (pattern, ty) in clause.core_patterns.iter().zip(&parameter_types) {
                    if let (Some(pattern), Some(ty)) = (pattern, ty) {
                        bind_pattern(pattern, ty, &mut variables);
                    }
                }
                if let Some(body) = clause.body.core_expr.as_mut() {
                    if let Some(return_type) = function.core_return_type.as_ref() {
                        specialize_expected_collection_new(
                            body,
                            return_type,
                            &function_types,
                            &core.module,
                        );
                        annotate_expected_structural_constructors(body, return_type);
                    }
                    specialize_expr(body, &variables, &function_types, &core.module);
                }
            }
        }
    }
}

pub(super) fn specialize_expr(
    expr: &mut CoreExpr,
    variables: &HashMap<String, CoreType>,
    functions: &FunctionTypes,
    module: &str,
) -> Option<CoreType> {
    match expr {
        CoreExpr::Int(_) => Some(CoreType::Int),
        CoreExpr::Float(_) => Some(CoreType::Float),
        CoreExpr::Binary(_) => Some(CoreType::String),
        CoreExpr::Atom(value) if matches!(value.as_str(), "true" | "false") => Some(CoreType::Bool),
        CoreExpr::Atom(value) if value == "Unit" => Some(CoreType::Named("Unit".to_string())),
        CoreExpr::Atom(_) => Some(CoreType::Atom),
        CoreExpr::Var(name) => variables.get(name).cloned(),
        CoreExpr::FieldAccess { base, field } | CoreExpr::RecordAccess { base, field, .. } => {
            let base_type = specialize_expr(base, variables, functions, module)?;
            named_field_type_with_nominals(&base_type, field, functions, module).cloned()
        }
        CoreExpr::RecordConstruct { name, fields } => {
            let nominal = CoreType::Named(name.clone());
            let Some(record_type) = nominal_type(functions, module, &nominal).cloned() else {
                for field in fields {
                    specialize_expr(&mut field.value, variables, functions, module);
                }
                return Some(nominal);
            };
            for field in fields {
                if let Some(expected) = named_field_type(&record_type, &field.key) {
                    specialize_expected_collection_new(
                        &mut field.value,
                        expected,
                        functions,
                        module,
                    );
                    annotate_expected_structural_constructors(&mut field.value, expected);
                }
                specialize_expr(&mut field.value, variables, functions, module);
            }
            Some(record_type)
        }
        CoreExpr::Tuple(items) => items
            .iter_mut()
            .map(|item| {
                specialize_expr(item, variables, functions, module)
                    .map(crate::terlan_typeck::CoreTupleTypeElem::Type)
            })
            .collect::<Option<Vec<_>>>()
            .map(CoreType::Tuple),
        CoreExpr::List(items) => {
            let element = items
                .iter_mut()
                .find_map(|item| specialize_expr(item, variables, functions, module))
                .unwrap_or(CoreType::Dynamic);
            Some(CoreType::List(Box::new(element)))
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            lift,
        } => specialize_comprehension(
            expr.as_mut(),
            generators,
            guards,
            lift,
            variables,
            functions,
            module,
        ),
        CoreExpr::ConstructorCall {
            constructor,
            constructor_identity,
            args,
        } if is_std_list_constructor(constructor, constructor_identity.as_deref()) => {
            let element = args
                .iter_mut()
                .find_map(|item| specialize_expr(item, variables, functions, module));
            let items = CoreExpr::List(std::mem::take(args));
            let Some(element) = element else {
                *expr = items;
                return None;
            };
            let list_type = CoreType::List(Box::new(element));
            *expr = CoreExpr::Cast {
                expr: Box::new(items),
                target_type: list_type.clone(),
            };
            Some(list_type)
        }
        CoreExpr::ConstructorCall {
            constructor,
            constructor_identity,
            args,
        } if is_std_set_constructor(constructor, constructor_identity.as_deref()) => {
            let element = args
                .iter_mut()
                .find_map(|item| specialize_expr(item, variables, functions, module))
                .unwrap_or_else(|| CoreType::Named("Unit".to_string()));
            let set_type = CoreType::Apply {
                constructor: "Set".to_string(),
                args: vec![element],
            };
            let values = CoreExpr::List(std::mem::take(args));
            *expr = CoreExpr::Intrinsic(CoreIntrinsicCall {
                id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetFromList),
                args: vec![values],
                return_type: set_type.clone(),
                effects: CoreEffectSet {
                    effects: Vec::new(),
                },
                span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
            });
            Some(set_type)
        }
        CoreExpr::ConstructorCall {
            constructor,
            constructor_identity,
            args,
        } if is_std_map_constructor(constructor, constructor_identity.as_deref()) => {
            let entry_types = args
                .iter_mut()
                .map(|entry| specialize_expr(entry, variables, functions, module))
                .collect::<Option<Vec<_>>>()?;
            let (key, value) = entry_types
                .first()
                .and_then(tuple_elements)
                .map(|(key, value)| (key.clone(), value.clone()))?;
            if !entry_types.iter().all(|entry| {
                tuple_elements(entry).is_some_and(|(entry_key, entry_value)| {
                    entry_key == &key && entry_value == &value
                })
            }) {
                return None;
            }
            let map_type = CoreType::Apply {
                constructor: "Map".to_string(),
                args: vec![key, value],
            };
            let constructor = std::mem::replace(expr, CoreExpr::Atom("Unit".to_string()));
            *expr = CoreExpr::Cast {
                expr: Box::new(constructor),
                target_type: map_type.clone(),
            };
            Some(map_type)
        }
        CoreExpr::ListCons { head, tail } => {
            let head = specialize_expr(head, variables, functions, module);
            specialize_expr(tail, variables, functions, module)
                .or_else(|| head.map(|head| CoreType::List(Box::new(head))))
        }
        CoreExpr::Index { base, index } => {
            let base_type = specialize_expr(base, variables, functions, module);
            specialize_expr(index, variables, functions, module);
            let element = base_type.as_ref().and_then(list_element)?.clone();
            let base = std::mem::replace(base, Box::new(CoreExpr::List(Vec::new())));
            let index = std::mem::replace(index, Box::new(CoreExpr::Int(0)));
            let args = vec![*base, *index];
            *expr = CoreExpr::Intrinsic(CoreIntrinsicCall {
                id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListGet),
                args,
                return_type: element.clone(),
                effects: CoreEffectSet {
                    effects: Vec::new(),
                },
                span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
            });
            Some(element)
        }
        CoreExpr::Call { function, args } => {
            let signature = function_signature(functions, module, function, args.len()).cloned();
            let argument_types = args
                .iter_mut()
                .map(|argument| specialize_expr(argument, variables, functions, module))
                .collect::<Vec<_>>();
            if let Some(signature) = signature.as_ref() {
                for (argument, expected) in args.iter_mut().zip(&signature.params) {
                    specialize_expected_collection_new(argument, expected, functions, module);
                    annotate_expected_structural_constructors(argument, expected);
                }
            }
            if function == "IndexGet.get_at" && args.len() == 2 {
                if let Some(element) = argument_types
                    .first()
                    .and_then(Option::as_ref)
                    .and_then(list_element)
                {
                    let return_type = element.clone();
                    *expr = CoreExpr::Intrinsic(CoreIntrinsicCall {
                        id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListGet),
                        args: std::mem::take(args),
                        return_type: return_type.clone(),
                        effects: CoreEffectSet {
                            effects: Vec::new(),
                        },
                        span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
                    });
                    return Some(return_type);
                }
            }
            if module == "std.collections.List" {
                if let Some(list) = argument_types
                    .first()
                    .and_then(Option::as_ref)
                    .and_then(list_element)
                {
                    if let Some(intrinsic) = list_receiver_intrinsic(function, args.len()) {
                        let return_type = list_intrinsic_return_type(list, &intrinsic);
                        *expr = CoreExpr::Intrinsic(CoreIntrinsicCall {
                            id: CoreIntrinsicId::Primitive(intrinsic),
                            args: std::mem::take(args),
                            return_type: return_type.clone(),
                            effects: CoreEffectSet {
                                effects: Vec::new(),
                            },
                            span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
                        });
                        return Some(return_type);
                    }
                }
            }
            if let Some(("std.collections.List", name)) = function.rsplit_once('.') {
                if let Some(list) = argument_types
                    .first()
                    .and_then(Option::as_ref)
                    .and_then(list_element)
                {
                    if let Some(intrinsic) = list_receiver_intrinsic(name, args.len()) {
                        let return_type = list_intrinsic_return_type(list, &intrinsic);
                        *expr = CoreExpr::Intrinsic(CoreIntrinsicCall {
                            id: CoreIntrinsicId::Primitive(intrinsic),
                            args: std::mem::take(args),
                            return_type: return_type.clone(),
                            effects: CoreEffectSet {
                                effects: Vec::new(),
                            },
                            span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
                        });
                        return Some(return_type);
                    }
                }
            }
            if let Some(set) = argument_types.first().and_then(Option::as_ref) {
                if set_element(set).is_some() {
                    let name = function.rsplit('.').next().unwrap_or(function);
                    if let Some(intrinsic) = set_receiver_intrinsic(name, args.len()) {
                        let return_type = set_intrinsic_return_type(&intrinsic, set);
                        *expr = CoreExpr::Intrinsic(CoreIntrinsicCall {
                            id: CoreIntrinsicId::Primitive(intrinsic),
                            args: std::mem::take(args),
                            return_type: return_type.clone(),
                            effects: CoreEffectSet {
                                effects: Vec::new(),
                            },
                            span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
                        });
                        return Some(return_type);
                    }
                }
            }
            function_return_type(functions, module, function, args.len())
        }
        CoreExpr::RemoteCall {
            module: owner,
            function,
            args,
        } => {
            let signature = functions
                .get(&(owner.clone(), function.clone(), args.len()))
                .cloned();
            let argument_types = args
                .iter_mut()
                .map(|argument| specialize_expr(argument, variables, functions, module))
                .collect::<Vec<_>>();
            if let Some(signature) = signature.as_ref() {
                for (argument, expected) in args.iter_mut().zip(&signature.params) {
                    specialize_expected_collection_new(argument, expected, functions, module);
                    annotate_expected_structural_constructors(argument, expected);
                }
            }
            if owner.rsplit('.').next() == Some("__receiver__") {
                if let Some(receiver) = argument_types.first().and_then(Option::as_ref) {
                    if matches!(receiver, CoreType::String) {
                        if let Some(intrinsic) =
                            crate::terlan_typeck::core_intrinsic_lowering::core_primitive_intrinsic(
                                "std.core.String",
                                function,
                                args.len(),
                            )
                        {
                            let return_type = core_primitive_intrinsic_return_type(&intrinsic);
                            *expr = CoreExpr::Intrinsic(CoreIntrinsicCall {
                                id: CoreIntrinsicId::Primitive(intrinsic),
                                args: std::mem::take(args),
                                return_type: return_type.clone(),
                                effects: CoreEffectSet {
                                    effects: Vec::new(),
                                },
                                span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
                            });
                            return Some(return_type);
                        }
                    } else if is_bitstring(receiver) {
                        if let Some(intrinsic) = bitstring_receiver_intrinsic(function, args.len())
                        {
                            let return_type = bitstring_intrinsic_return_type(&intrinsic);
                            *expr = CoreExpr::Intrinsic(CoreIntrinsicCall {
                                id: CoreIntrinsicId::Primitive(intrinsic),
                                args: std::mem::take(args),
                                return_type: return_type.clone(),
                                effects: CoreEffectSet {
                                    effects: Vec::new(),
                                },
                                span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
                            });
                            return Some(return_type);
                        }
                    } else if is_bytes(receiver) {
                        if let Some(intrinsic) = bytes_receiver_intrinsic(function, args.len()) {
                            let return_type = bytes_intrinsic_return_type(&intrinsic);
                            *expr = CoreExpr::Intrinsic(CoreIntrinsicCall {
                                id: CoreIntrinsicId::Primitive(intrinsic),
                                args: std::mem::take(args),
                                return_type: return_type.clone(),
                                effects: CoreEffectSet {
                                    effects: Vec::new(),
                                },
                                span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
                            });
                            return Some(return_type);
                        }
                    } else if map_elements(receiver).is_some() {
                        if let Some(intrinsic) = map_receiver_intrinsic(function, args.len()) {
                            let return_type = map_intrinsic_return_type(&intrinsic, receiver);
                            *expr = CoreExpr::Intrinsic(CoreIntrinsicCall {
                                id: CoreIntrinsicId::Primitive(intrinsic),
                                args: std::mem::take(args),
                                return_type: return_type.clone(),
                                effects: CoreEffectSet {
                                    effects: Vec::new(),
                                },
                                span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
                            });
                            return Some(return_type);
                        }
                    } else if set_element(receiver).is_some() {
                        if let Some(intrinsic) = set_receiver_intrinsic(function, args.len()) {
                            let return_type = set_intrinsic_return_type(&intrinsic, receiver);
                            *expr = CoreExpr::Intrinsic(CoreIntrinsicCall {
                                id: CoreIntrinsicId::Primitive(intrinsic),
                                args: std::mem::take(args),
                                return_type: return_type.clone(),
                                effects: CoreEffectSet {
                                    effects: Vec::new(),
                                },
                                span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
                            });
                            return Some(return_type);
                        }
                    } else if let Some(list) = list_element(receiver) {
                        if let Some(intrinsic) = list_receiver_intrinsic(function, args.len()) {
                            let return_type = list_intrinsic_return_type(list, &intrinsic);
                            *expr = CoreExpr::Intrinsic(CoreIntrinsicCall {
                                id: CoreIntrinsicId::Primitive(intrinsic),
                                args: std::mem::take(args),
                                return_type: return_type.clone(),
                                effects: CoreEffectSet {
                                    effects: Vec::new(),
                                },
                                span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
                            });
                            return Some(return_type);
                        }
                    }
                }
            }
            if owner == "std.collections.List" {
                if let Some(list) = argument_types
                    .first()
                    .and_then(Option::as_ref)
                    .and_then(list_element)
                {
                    if let Some(intrinsic) = list_receiver_intrinsic(function, args.len()) {
                        let return_type = list_intrinsic_return_type(list, &intrinsic);
                        *expr = CoreExpr::Intrinsic(CoreIntrinsicCall {
                            id: CoreIntrinsicId::Primitive(intrinsic),
                            args: std::mem::take(args),
                            return_type: return_type.clone(),
                            effects: CoreEffectSet {
                                effects: Vec::new(),
                            },
                            span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
                        });
                        return Some(return_type);
                    }
                }
            }
            if owner == "std.collections.Set" {
                if let Some(set) = argument_types.first().and_then(Option::as_ref) {
                    if set_element(set).is_some() {
                        if let Some(intrinsic) = set_receiver_intrinsic(function, args.len()) {
                            let return_type = set_intrinsic_return_type(&intrinsic, set);
                            *expr = CoreExpr::Intrinsic(CoreIntrinsicCall {
                                id: CoreIntrinsicId::Primitive(intrinsic),
                                args: std::mem::take(args),
                                return_type: return_type.clone(),
                                effects: CoreEffectSet {
                                    effects: Vec::new(),
                                },
                                span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
                            });
                            return Some(return_type);
                        }
                    }
                }
            }
            signature.map(|signature| signature.result)
        }
        CoreExpr::MutableReceiverCall {
            receiver,
            method,
            args,
            effects,
        } => {
            let receiver_type = specialize_expr(receiver, variables, functions, module);
            for argument in args.iter_mut() {
                specialize_expr(argument, variables, functions, module);
            }
            let arity = args.len() + 1;
            if let Some(map) = receiver_type
                .as_ref()
                .filter(|ty| map_elements(ty).is_some())
            {
                let intrinsic = map_receiver_intrinsic(method, arity)?;
                let mut operands = Vec::with_capacity(arity);
                operands.push((**receiver).clone());
                operands.append(args);
                *expr = CoreExpr::Intrinsic(CoreIntrinsicCall {
                    id: CoreIntrinsicId::Primitive(intrinsic),
                    args: operands,
                    return_type: map.clone(),
                    effects: effects.clone(),
                    span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
                });
                return Some(map.clone());
            }
            if let Some(set) = receiver_type
                .as_ref()
                .filter(|ty| set_element(ty).is_some())
            {
                let intrinsic = set_receiver_intrinsic(method, arity)?;
                let mut operands = Vec::with_capacity(arity);
                operands.push((**receiver).clone());
                operands.append(args);
                *expr = CoreExpr::Intrinsic(CoreIntrinsicCall {
                    id: CoreIntrinsicId::Primitive(intrinsic),
                    args: operands,
                    return_type: set.clone(),
                    effects: effects.clone(),
                    span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
                });
                return Some(set.clone());
            }
            let list = receiver_type.as_ref().and_then(|ty| list_element(ty))?;
            let intrinsic = list_receiver_intrinsic(method, arity)?;
            let return_type = list_intrinsic_return_type(list, &intrinsic);
            let mut operands = Vec::with_capacity(arity);
            operands.push((**receiver).clone());
            operands.append(args);
            *expr = CoreExpr::Intrinsic(CoreIntrinsicCall {
                id: CoreIntrinsicId::Primitive(intrinsic),
                args: operands,
                return_type: return_type.clone(),
                effects: effects.clone(),
                span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
            });
            Some(return_type)
        }
        CoreExpr::Intrinsic(call) => {
            let argument_types = call
                .args
                .iter_mut()
                .map(|argument| specialize_expr(argument, variables, functions, module))
                .collect::<Vec<_>>();
            if let CoreIntrinsicId::Primitive(intrinsic) = &call.id {
                if *intrinsic == CorePrimitiveIntrinsic::SetNew
                    && set_element(&call.return_type).is_some_and(is_dynamic_type)
                {
                    // An unconstrained empty set has no observable element
                    // representation. Give it the zero-sized Unit witness so
                    // direct AOT retains a closed schema without inventing a
                    // dynamic managed-field ABI. Any typed consumer or later
                    // mutation has already retargeted SetNew above.
                    call.return_type = CoreType::Apply {
                        constructor: "Set".to_string(),
                        args: vec![CoreType::Named("Unit".to_string())],
                    };
                }
                if let Some(list) = argument_types.first().and_then(Option::as_ref) {
                    if let Some(element) = list_element(list) {
                        call.return_type = match intrinsic {
                            CorePrimitiveIntrinsic::ListGet => element.clone(),
                            CorePrimitiveIntrinsic::ListFirst => CoreType::Apply {
                                constructor: "Option".to_string(),
                                args: vec![element.clone()],
                            },
                            CorePrimitiveIntrinsic::ListRest => CoreType::Apply {
                                constructor: "Option".to_string(),
                                args: vec![list.clone()],
                            },
                            CorePrimitiveIntrinsic::ListIterator => CoreType::Apply {
                                constructor: "Iterator".to_string(),
                                args: vec![element.clone()],
                            },
                            CorePrimitiveIntrinsic::ListConcat
                            | CorePrimitiveIntrinsic::ListSubtract
                            | CorePrimitiveIntrinsic::ListPush
                            | CorePrimitiveIntrinsic::ListClear => list.clone(),
                            _ => call.return_type.clone(),
                        };
                        if *intrinsic == CorePrimitiveIntrinsic::MapFromEntries {
                            if let Some((key, value)) = tuple_elements(element) {
                                call.return_type = CoreType::Apply {
                                    constructor: "Map".to_string(),
                                    args: vec![key.clone(), value.clone()],
                                };
                            }
                        }
                        if *intrinsic == CorePrimitiveIntrinsic::SetFromList {
                            call.return_type = CoreType::Apply {
                                constructor: "Set".to_string(),
                                args: vec![element.clone()],
                            };
                        }
                    }
                }
                if let Some(map) = argument_types.first().and_then(Option::as_ref) {
                    if let Some((key, value)) = map_elements(map) {
                        call.return_type = match intrinsic {
                            CorePrimitiveIntrinsic::MapGet => option(value.clone()),
                            CorePrimitiveIntrinsic::MapTake => CoreType::Tuple(vec![
                                crate::terlan_typeck::CoreTupleTypeElem::Type(option(
                                    value.clone(),
                                )),
                                crate::terlan_typeck::CoreTupleTypeElem::Type(map.clone()),
                            ]),
                            CorePrimitiveIntrinsic::MapIterator => CoreType::Apply {
                                constructor: "Iterator".to_string(),
                                args: vec![CoreType::Tuple(vec![
                                    crate::terlan_typeck::CoreTupleTypeElem::Type(key.clone()),
                                    crate::terlan_typeck::CoreTupleTypeElem::Type(value.clone()),
                                ])],
                            },
                            CorePrimitiveIntrinsic::MapPut
                            | CorePrimitiveIntrinsic::MapRemove
                            | CorePrimitiveIntrinsic::MapClear => map.clone(),
                            _ => call.return_type.clone(),
                        };
                    }
                }
                if let Some(set) = argument_types.first().and_then(Option::as_ref) {
                    if let Some(element) = set_element(set) {
                        call.return_type = match intrinsic {
                            CorePrimitiveIntrinsic::SetIsEmpty
                            | CorePrimitiveIntrinsic::SetContains => CoreType::Bool,
                            CorePrimitiveIntrinsic::SetSize => CoreType::Int,
                            CorePrimitiveIntrinsic::SetIterator => CoreType::Apply {
                                constructor: "Iterator".to_string(),
                                args: vec![element.clone()],
                            },
                            CorePrimitiveIntrinsic::SetAdd
                            | CorePrimitiveIntrinsic::SetRemove
                            | CorePrimitiveIntrinsic::SetClear => set.clone(),
                            _ => call.return_type.clone(),
                        };
                    }
                }
                if *intrinsic == CorePrimitiveIntrinsic::IteratorNext {
                    if let Some(iterator) = argument_types.first().and_then(Option::as_ref) {
                        if let Some(element) = iterator_element(iterator) {
                            call.return_type = option(CoreType::Map(vec![
                                crate::terlan_typeck::CoreMapTypeField {
                                    key: "value".to_string(),
                                    operator: ":".to_string(),
                                    value: element.clone(),
                                },
                                crate::terlan_typeck::CoreMapTypeField {
                                    key: "next".to_string(),
                                    operator: ":".to_string(),
                                    value: iterator.clone(),
                                },
                            ]));
                        }
                    }
                }
            }
            Some(call.return_type.clone())
        }
        CoreExpr::Let { bindings, body } => {
            specialize_collection_new_bindings(bindings, body, functions, module);
            let mut variables = variables.clone();
            let mut binding_index = 0;
            while binding_index < bindings.len() {
                let unit_result_pattern = functionalize_collection_receiver_binding(
                    &mut bindings[binding_index],
                    &variables,
                );
                let binding = &mut bindings[binding_index];
                let ty = specialize_expr(&mut binding.value, &variables, functions, module);
                if let Some(ty) = ty {
                    bind_pattern(&binding.pattern, &ty, &mut variables);
                }
                if let Some(pattern) = unit_result_pattern {
                    bindings.insert(
                        binding_index + 1,
                        CoreLetBinding {
                            pattern,
                            value: CoreExpr::Atom("Unit".to_string()),
                        },
                    );
                }
                binding_index += 1;
            }
            specialize_expr(body, &variables, functions, module)
        }
        CoreExpr::Case { scrutinee, clauses } => {
            let scrutinee_type = specialize_expr(scrutinee, variables, functions, module);
            if let Some(ty) = scrutinee_type.as_ref() {
                if matches!(
                    scrutinee.as_ref(),
                    CoreExpr::Call { .. } | CoreExpr::Tuple(_)
                ) {
                    let call =
                        std::mem::replace(scrutinee.as_mut(), CoreExpr::Atom("Unit".to_string()));
                    **scrutinee = CoreExpr::Cast {
                        expr: Box::new(call),
                        target_type: ty.clone(),
                    };
                }
            }
            let mut result = None;
            for clause in clauses {
                let mut variables = variables.clone();
                if let Some(scrutinee_type) = scrutinee_type.as_ref() {
                    bind_pattern(&clause.pattern, scrutinee_type, &mut variables);
                }
                if let Some(guard) = clause.guard.as_mut() {
                    specialize_expr(guard, &variables, functions, module);
                }
                let branch = specialize_expr(&mut clause.body, &variables, functions, module);
                if result.is_none() {
                    result = branch;
                }
            }
            result
        }
        CoreExpr::If { clauses } => {
            let mut result = None;
            for clause in clauses {
                specialize_expr(&mut clause.condition, variables, functions, module);
                let branch = specialize_expr(&mut clause.body, variables, functions, module);
                if result.is_none() {
                    result = branch;
                }
            }
            result
        }
        CoreExpr::Cast { expr, target_type } => {
            specialize_expr(expr, variables, functions, module);
            Some(target_type.clone())
        }
        CoreExpr::UnaryOp { operand, .. } => specialize_expr(operand, variables, functions, module),
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => {
            let left_type = specialize_expr(left, variables, functions, module);
            let right_type = specialize_expr(right, variables, functions, module);
            if operator == ".." {
                return Some(CoreType::Named("std.range.Range.Range".to_string()));
            }
            if operator == "in" {
                return Some(CoreType::Bool);
            }
            if matches!(operator.as_str(), "==" | "!=") {
                if let Some(expected) = right_type.as_ref() {
                    specialize_expected_collection_new(left, expected, functions, module);
                    annotate_expected_structural_constructors(left, expected);
                }
                if let Some(expected) = left_type.as_ref() {
                    specialize_expected_collection_new(right, expected, functions, module);
                    annotate_expected_structural_constructors(right, expected);
                }
            }
            if matches!(
                operator.as_str(),
                "==" | "!=" | "<" | "<=" | ">" | ">=" | "and" | "or"
            ) {
                Some(CoreType::Bool)
            } else {
                left_type.or(right_type)
            }
        }
        _ => {
            visit_children(expr, variables, functions, module);
            None
        }
    }
}

/// Functionalizes one mutable collection receiver call for the persistent AOT
/// heap while retaining the source-level `Unit` result.
///
/// A source binding such as `_result = values.push(value)` mutates `values`
/// conceptually. The native managed heap instead returns a new persistent
/// collection. Bind that new value to `values`, then insert a `Unit` binding
/// for `_result`. Sequence lowering already binds discarded receiver calls
/// directly to the receiver and therefore needs no extra binding.
fn functionalize_collection_receiver_binding(
    binding: &mut CoreLetBinding,
    variables: &HashMap<String, CoreType>,
) -> Option<CorePattern> {
    let CoreExpr::MutableReceiverCall {
        receiver,
        method,
        args,
        ..
    } = &binding.value
    else {
        return None;
    };
    let CoreExpr::Var(receiver_name) = receiver.as_ref() else {
        return None;
    };
    let receiver_type = variables.get(receiver_name)?;
    let arity = args.len() + 1;
    let is_persistent_mutator = if map_elements(receiver_type).is_some() {
        matches!(
            map_receiver_intrinsic(method, arity),
            Some(
                CorePrimitiveIntrinsic::MapPut
                    | CorePrimitiveIntrinsic::MapRemove
                    | CorePrimitiveIntrinsic::MapClear
            )
        )
    } else if set_element(receiver_type).is_some() {
        matches!(
            set_receiver_intrinsic(method, arity),
            Some(
                CorePrimitiveIntrinsic::SetAdd
                    | CorePrimitiveIntrinsic::SetRemove
                    | CorePrimitiveIntrinsic::SetClear
            )
        )
    } else if list_element(receiver_type).is_some() {
        matches!(
            list_receiver_intrinsic(method, arity),
            Some(CorePrimitiveIntrinsic::ListPush | CorePrimitiveIntrinsic::ListClear)
        )
    } else {
        false
    };
    if !is_persistent_mutator {
        return None;
    }
    if matches!(&binding.pattern, CorePattern::Var(name) if name == receiver_name) {
        return None;
    }
    Some(std::mem::replace(
        &mut binding.pattern,
        CorePattern::Var(receiver_name.clone()),
    ))
}

mod support;

use support::*;
