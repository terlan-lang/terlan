//! Restores concrete generic collection result types before NativeIR lowering.

use std::collections::HashMap;

use crate::terlan_typeck::{
    core_type_from_text, CoreEffectSet, CoreExpr, CoreIntrinsicCall, CoreIntrinsicId,
    CoreLetBinding, CoreModule, CorePattern, CorePrimitiveIntrinsic, CoreType,
};

#[path = "collection_intrinsic_specialization/expected_constructors.rs"]
mod expected_constructors;
use expected_constructors::annotate_expected_structural_constructors;

#[derive(Clone)]
pub(super) struct FunctionSignature {
    pub(super) params: Vec<CoreType>,
    pub(super) result: CoreType,
}

pub(super) type FunctionTypes = HashMap<(String, String, usize), FunctionSignature>;

pub(super) fn specialize_collection_intrinsic_results(cores: &mut [CoreModule]) {
    let function_types = cores
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
    for core in cores {
        for function in &mut core.functions {
            let variables = function
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
                if let Some(body) = clause.body.core_expr.as_mut() {
                    if let Some(return_type) = function.core_return_type.as_ref() {
                        specialize_expected_collection_new(body, return_type);
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
                    specialize_expected_collection_new(argument, expected);
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
                    specialize_expected_collection_new(argument, expected);
                    annotate_expected_structural_constructors(argument, expected);
                }
            }
            if owner.rsplit('.').next() == Some("__receiver__") {
                if let Some(receiver) = argument_types.first().and_then(Option::as_ref) {
                    if matches!(receiver, CoreType::String)
                        && function == "is_empty"
                        && args.len() == 1
                    {
                        let value = args
                            .pop()
                            .expect("string receiver call has one checked argument");
                        *expr = CoreExpr::BinaryOp {
                            operator: "==".to_string(),
                            left: Box::new(value),
                            right: Box::new(CoreExpr::Binary(String::new())),
                        };
                        return Some(CoreType::Bool);
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
                let Some(intrinsic) = map_receiver_intrinsic(method, arity) else {
                    return None;
                };
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
            let Some(list) = receiver_type.as_ref().and_then(|ty| list_element(ty)) else {
                return None;
            };
            let Some(intrinsic) = list_receiver_intrinsic(method, arity) else {
                return None;
            };
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
                        std::mem::replace(scrutinee, Box::new(CoreExpr::Atom("Unit".to_string())));
                    *scrutinee = Box::new(CoreExpr::Cast {
                        expr: call,
                        target_type: ty.clone(),
                    });
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
            if matches!(operator.as_str(), "==" | "!=") {
                if let Some(expected) = right_type.as_ref() {
                    annotate_expected_structural_constructors(left, expected);
                }
                if let Some(expected) = left_type.as_ref() {
                    annotate_expected_structural_constructors(right, expected);
                }
            }
            if matches!(
                operator.as_str(),
                "==" | "!=" | "<" | "<=" | ">" | ">=" | "and" | "&&" | "or" | "||"
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

fn is_std_list_constructor(constructor: &str, identity: Option<&str>) -> bool {
    constructor == "std.collections.List.List"
        || identity.is_some_and(|identity| {
            identity == "std.collections.List.List"
                || identity.starts_with("std.collections.List.List/")
        })
}

include!("collection_intrinsic_specialization/support.rs");

fn option_element(ty: &CoreType) -> Option<&CoreType> {
    match ty {
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Option") && args.len() == 1 =>
        {
            Some(&args[0])
        }
        CoreType::Union(variants)
            if variants.iter().any(
                |variant| matches!(variant, CoreType::AtomLiteral(atom) if atom == "none"),
            ) =>
        {
            variants.iter().find_map(|variant| {
                let CoreType::Tuple(elements) = variant else {
                    return None;
                };
                let [tag, value] = elements.as_slice() else {
                    return None;
                };
                matches!(tuple_element_type(tag), CoreType::AtomLiteral(atom) if atom == "some")
                    .then(|| tuple_element_type(value))
            })
        }
        _ => None,
    }
}

fn tuple_elements(ty: &CoreType) -> Option<(&CoreType, &CoreType)> {
    let CoreType::Tuple(elements) = ty else {
        return None;
    };
    let [left, right] = elements.as_slice() else {
        return None;
    };
    Some((tuple_element_type(left), tuple_element_type(right)))
}

fn tuple_element_type(element: &crate::terlan_typeck::CoreTupleTypeElem) -> &CoreType {
    match element {
        crate::terlan_typeck::CoreTupleTypeElem::Type(ty)
        | crate::terlan_typeck::CoreTupleTypeElem::Field { ty, .. } => ty,
    }
}

fn map_elements(ty: &CoreType) -> Option<(&CoreType, &CoreType)> {
    match ty {
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Map") && args.len() == 2 =>
        {
            Some((&args[0], &args[1]))
        }
        _ => None,
    }
}

fn option(element: CoreType) -> CoreType {
    CoreType::Apply {
        constructor: "Option".to_string(),
        args: vec![element],
    }
}

fn specialize_expected_collection_new(expr: &mut CoreExpr, expected: &CoreType) {
    let CoreExpr::Intrinsic(call) = expr else {
        return;
    };
    match call.id {
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapNew)
            if map_elements(expected).is_some() =>
        {
            call.return_type = expected.clone();
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListNew)
            if list_element(expected).is_some() =>
        {
            call.return_type = expected.clone();
        }
        _ => {}
    }
}

fn specialize_collection_new_bindings(
    bindings: &mut [CoreLetBinding],
    body: &CoreExpr,
    functions: &FunctionTypes,
    module: &str,
) {
    for index in 0..bindings.len() {
        let CorePattern::Var(name) = &bindings[index].pattern else {
            continue;
        };
        let name = name.clone();
        let inferred = bindings[index + 1..]
            .iter()
            .find_map(|binding| {
                expected_call_argument_type(&name, &binding.value, functions, module)
                    .or_else(|| infer_map_put(&name, &binding.value))
            })
            .or_else(|| expected_call_argument_type(&name, body, functions, module))
            .or_else(|| infer_map_put(&name, body));
        let Some(inferred) = inferred else {
            continue;
        };
        if let CoreExpr::Intrinsic(call) = &mut bindings[index].value {
            match call.id {
                CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapNew)
                    if map_elements(&inferred).is_some() =>
                {
                    call.return_type = inferred;
                }
                CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListNew)
                    if list_element(&inferred).is_some() =>
                {
                    call.return_type = inferred;
                }
                _ => {}
            }
        }
    }
}

fn expected_call_argument_type(
    name: &str,
    expr: &CoreExpr,
    functions: &FunctionTypes,
    module: &str,
) -> Option<CoreType> {
    let (signature, args) = match expr {
        CoreExpr::Call { function, args } => (
            function_signature(functions, module, function, args.len()),
            args,
        ),
        CoreExpr::RemoteCall {
            module: owner,
            function,
            args,
        } => (
            functions.get(&(owner.clone(), function.clone(), args.len())),
            args,
        ),
        CoreExpr::Cast { expr, .. } => {
            return expected_call_argument_type(name, expr, functions, module);
        }
        _ => return None,
    };
    let signature = signature?;
    args.iter()
        .zip(&signature.params)
        .find_map(|(argument, expected)| {
            matches!(argument, CoreExpr::Var(argument) if argument == name)
                .then(|| expected.clone())
        })
}

fn infer_map_put(name: &str, expr: &CoreExpr) -> Option<CoreType> {
    match expr {
        CoreExpr::MutableReceiverCall {
            receiver,
            method,
            args,
            ..
        } if method == "put"
            && matches!(receiver.as_ref(), CoreExpr::Var(receiver) if receiver == name) =>
        {
            let [key, value] = args.as_slice() else {
                return None;
            };
            Some(CoreType::Apply {
                constructor: "Map".to_string(),
                args: vec![literal_type(key)?, literal_type(value)?],
            })
        }
        CoreExpr::Let { bindings, body } => bindings
            .iter()
            .find_map(|binding| infer_map_put(name, &binding.value))
            .or_else(|| infer_map_put(name, body)),
        _ => None,
    }
}

fn literal_type(expr: &CoreExpr) -> Option<CoreType> {
    match expr {
        CoreExpr::Int(_) => Some(CoreType::Int),
        CoreExpr::Float(_) => Some(CoreType::Float),
        CoreExpr::Binary(_) => Some(CoreType::String),
        CoreExpr::Atom(value) if matches!(value.as_str(), "true" | "false") => Some(CoreType::Bool),
        CoreExpr::Atom(_) => Some(CoreType::Atom),
        _ => None,
    }
}

fn map_receiver_intrinsic(method: &str, arity: usize) -> Option<CorePrimitiveIntrinsic> {
    match (method, arity) {
        ("is_empty", 1) => Some(CorePrimitiveIntrinsic::MapIsEmpty),
        ("size", 1) => Some(CorePrimitiveIntrinsic::MapSize),
        ("get", 2) => Some(CorePrimitiveIntrinsic::MapGet),
        ("contains_key", 2) => Some(CorePrimitiveIntrinsic::MapContainsKey),
        ("iterator", 1) => Some(CorePrimitiveIntrinsic::MapIterator),
        ("put", 3) => Some(CorePrimitiveIntrinsic::MapPut),
        ("remove", 2) => Some(CorePrimitiveIntrinsic::MapRemove),
        ("clear", 1) => Some(CorePrimitiveIntrinsic::MapClear),
        _ => None,
    }
}

fn list_receiver_intrinsic(method: &str, arity: usize) -> Option<CorePrimitiveIntrinsic> {
    match (method, arity) {
        ("is_empty", 1) => Some(CorePrimitiveIntrinsic::ListIsEmpty),
        ("length", 1) => Some(CorePrimitiveIntrinsic::ListLength),
        ("first", 1) => Some(CorePrimitiveIntrinsic::ListFirst),
        ("rest", 1) => Some(CorePrimitiveIntrinsic::ListRest),
        ("iterator", 1) => Some(CorePrimitiveIntrinsic::ListIterator),
        ("push", 2) => Some(CorePrimitiveIntrinsic::ListPush),
        ("clear", 1) => Some(CorePrimitiveIntrinsic::ListClear),
        _ => None,
    }
}

fn bytes_receiver_intrinsic(method: &str, arity: usize) -> Option<CorePrimitiveIntrinsic> {
    match (method, arity) {
        ("to_list", 1) => Some(CorePrimitiveIntrinsic::VmBytesToList),
        ("length", 1) => Some(CorePrimitiveIntrinsic::VmBytesLength),
        ("slice", 3) => Some(CorePrimitiveIntrinsic::VmBytesSlice),
        ("read_uint_be", 3) => Some(CorePrimitiveIntrinsic::VmBytesReadUintBe),
        ("read_int_be", 3) => Some(CorePrimitiveIntrinsic::VmBytesReadIntBe),
        ("read_uint_le", 3) => Some(CorePrimitiveIntrinsic::VmBytesReadUintLe),
        ("read_int_le", 3) => Some(CorePrimitiveIntrinsic::VmBytesReadIntLe),
        _ => None,
    }
}

fn bitstring_receiver_intrinsic(method: &str, arity: usize) -> Option<CorePrimitiveIntrinsic> {
    match (method, arity) {
        ("to_utf8_scalar", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToUtf8Scalar),
        ("to_utf16_be_scalar", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToUtf16BeScalar),
        ("to_utf16_le_scalar", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToUtf16LeScalar),
        ("to_utf32_be_scalar", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToUtf32BeScalar),
        ("to_utf32_le_scalar", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToUtf32LeScalar),
        ("bit_length", 1) => Some(CorePrimitiveIntrinsic::VmBitStringBitLength),
        ("byte_length", 1) => Some(CorePrimitiveIntrinsic::VmBitStringByteLength),
        ("is_byte_aligned", 1) => Some(CorePrimitiveIntrinsic::VmBitStringIsByteAligned),
        ("slice", 3) => Some(CorePrimitiveIntrinsic::VmBitStringSlice),
        ("concat", 2) => Some(CorePrimitiveIntrinsic::VmBitStringConcat),
        ("to_bytes", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToBytes),
        ("to_uint_be", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToUintBe),
        ("to_int_be", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToIntBe),
        ("to_uint_le", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToUintLe),
        ("to_int_le", 1) => Some(CorePrimitiveIntrinsic::VmBitStringToIntLe),
        _ => None,
    }
}

fn bitstring_intrinsic_return_type(intrinsic: &CorePrimitiveIntrinsic) -> CoreType {
    match intrinsic {
        CorePrimitiveIntrinsic::VmBitStringBitLength
        | CorePrimitiveIntrinsic::VmBitStringByteLength
        | CorePrimitiveIntrinsic::VmBitStringToUtf8Scalar
        | CorePrimitiveIntrinsic::VmBitStringToUtf16BeScalar
        | CorePrimitiveIntrinsic::VmBitStringToUtf16LeScalar
        | CorePrimitiveIntrinsic::VmBitStringToUtf32BeScalar
        | CorePrimitiveIntrinsic::VmBitStringToUtf32LeScalar
        | CorePrimitiveIntrinsic::VmBitStringToUintBe
        | CorePrimitiveIntrinsic::VmBitStringToIntBe
        | CorePrimitiveIntrinsic::VmBitStringToUintLe
        | CorePrimitiveIntrinsic::VmBitStringToIntLe => CoreType::Int,
        CorePrimitiveIntrinsic::VmBitStringIsByteAligned => CoreType::Bool,
        CorePrimitiveIntrinsic::VmBitStringToBytes => CoreType::Named("Bytes".to_string()),
        _ => CoreType::Named("BitString".to_string()),
    }
}

fn bytes_intrinsic_return_type(intrinsic: &CorePrimitiveIntrinsic) -> CoreType {
    match intrinsic {
        CorePrimitiveIntrinsic::VmBytesToList => CoreType::List(Box::new(CoreType::Int)),
        CorePrimitiveIntrinsic::VmBytesLength
        | CorePrimitiveIntrinsic::VmBytesReadUintBe
        | CorePrimitiveIntrinsic::VmBytesReadIntBe
        | CorePrimitiveIntrinsic::VmBytesReadUintLe
        | CorePrimitiveIntrinsic::VmBytesReadIntLe => CoreType::Int,
        _ => CoreType::Named("Bytes".to_string()),
    }
}

pub(super) fn list_intrinsic_return_type(
    element: &CoreType,
    intrinsic: &CorePrimitiveIntrinsic,
) -> CoreType {
    match intrinsic {
        CorePrimitiveIntrinsic::ListIsEmpty => CoreType::Bool,
        CorePrimitiveIntrinsic::ListLength => CoreType::Int,
        CorePrimitiveIntrinsic::ListGet => element.clone(),
        CorePrimitiveIntrinsic::ListFirst => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![element.clone()],
        },
        CorePrimitiveIntrinsic::ListRest => CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::List(Box::new(element.clone()))],
        },
        CorePrimitiveIntrinsic::ListIterator => CoreType::Apply {
            constructor: "Iterator".to_string(),
            args: vec![element.clone()],
        },
        _ => CoreType::List(Box::new(element.clone())),
    }
}

fn map_intrinsic_return_type(intrinsic: &CorePrimitiveIntrinsic, map: &CoreType) -> CoreType {
    let Some((key, value)) = map_elements(map) else {
        return map.clone();
    };
    match intrinsic {
        CorePrimitiveIntrinsic::MapIsEmpty | CorePrimitiveIntrinsic::MapContainsKey => {
            CoreType::Bool
        }
        CorePrimitiveIntrinsic::MapSize => CoreType::Int,
        CorePrimitiveIntrinsic::MapGet => option(value.clone()),
        CorePrimitiveIntrinsic::MapIterator => CoreType::Apply {
            constructor: "Iterator".to_string(),
            args: vec![CoreType::Tuple(vec![
                crate::terlan_typeck::CoreTupleTypeElem::Type(key.clone()),
                crate::terlan_typeck::CoreTupleTypeElem::Type(value.clone()),
            ])],
        },
        _ => map.clone(),
    }
}
