//! Resolves checked function and nominal types at the native ABI boundary.

use super::*;
use crate::compiler::native_ir::expression::infer_native_type;

/// Resolves the scalar ABI result for a native candidate.
///
/// Ordinary functions keep their checked declared type. A `Dynamic` wrapper,
/// such as the synthetic REPL entry, may narrow only when its typed CoreIR body
/// is independently scalar and does not need another function's result type.
/// This preserves the source-facing dynamic contract while allowing the AOT
/// boundary to carry the concrete `Unit`, `Int`, `Float`, or `Bool` value it
/// proved.
pub(in crate::compiler::native_ir) fn native_return_type(
    function: &CoreFunction,
) -> Option<NativeType> {
    native_type(function.core_return_type.as_ref(), &function.return_type)
        .or_else(|| {
            let inferred = dynamic_return::inferred_dynamic_return_type(function)?;
            native_type(Some(&inferred), &inferred.contract_text())
        })
        .or_else(|| {
            let variables = function
                .params
                .iter()
                .map(|param| {
                    native_type(param.core_ty.as_ref(), &param.ty)
                        .map(|ty| (param.name.clone(), ty))
                })
                .collect::<Option<HashMap<_, _>>>()?;
            let body = function.clauses.first()?.body.core_expr.as_ref()?;
            infer_native_type(body, &variables, &HashMap::new())
        })
}

pub(in crate::compiler::native_ir) fn native_return_type_with_constructors(
    function: &CoreFunction,
    constructors: &NativeConstructorLayouts,
) -> Option<NativeType> {
    native_type_with_constructors(
        function.core_return_type.as_ref(),
        &function.return_type,
        constructors,
    )
    .or_else(|| native_return_type(function))
    .or_else(|| {
        matches!(
            function.core_return_type.as_ref(),
            Some(crate::terlan_typeck::CoreType::Dynamic)
        )
        .then_some(())?;
        let variables = function
            .params
            .iter()
            .map(|param| {
                native_type_with_constructors(param.core_ty.as_ref(), &param.ty, constructors)
                    .map(|ty| (param.name.clone(), ty))
            })
            .collect::<Option<HashMap<_, _>>>()?;
        let body = function.clauses.first()?.body.core_expr.as_ref()?;
        infer_native_type_with_constructors(body, &variables, &HashMap::new(), constructors)
    })
}

pub(in crate::compiler::native_ir) fn native_type_with_constructors(
    core: Option<&crate::terlan_typeck::CoreType>,
    text: &str,
    constructors: &NativeConstructorLayouts,
) -> Option<NativeType> {
    let refined_core = core
        .filter(|core| core_type_contains_dynamic(core))
        .and_then(|_| concrete_textual_core_type(text, constructors));
    let core = refined_core.as_ref().or(core);
    if let Some(core @ CoreType::Apply { .. }) = core {
        let canonical = core.contract_text();
        if let Some(layout) = constructors
            .values()
            .find(|layout| layout.descriptor.canonical_type() == canonical)
        {
            return Some(layout.result);
        }
    }
    if let Some(crate::terlan_typeck::CoreType::Named(name)) = core {
        let mut matching = constructors
            .iter()
            .filter(|((identity, _), _)| identity == name)
            .map(|(_, layout)| layout.result);
        if let Some(result) = matching.next() {
            if matching.all(|candidate| candidate == result) {
                return Some(result);
            }
        }
    }
    native_type(core, text)
}

/// Recovers concrete imported record types erased to `Dynamic` inside a
/// collection/function annotation. Runtime collection identities include the
/// structural element contract, so retaining `List(Dynamic)` for a source
/// `List[Payload]` would make an AOT continuation reject the actual list.
fn concrete_textual_core_type(
    text: &str,
    constructors: &NativeConstructorLayouts,
) -> Option<CoreType> {
    let mut recovered = crate::terlan_typeck::core_type_from_text(text)?;
    resolve_constructor_types(&mut recovered, constructors);
    (!core_type_contains_dynamic(&recovered)).then_some(recovered)
}

fn resolve_constructor_types(ty: &mut CoreType, constructors: &NativeConstructorLayouts) {
    match ty {
        CoreType::Named(name) => {
            let replacement = unique_constructor_core_type(name, constructors);
            if let Some(replacement) = replacement {
                *ty = replacement;
            }
        }
        CoreType::Apply { args, .. } => args
            .iter_mut()
            .for_each(|ty| resolve_constructor_types(ty, constructors)),
        CoreType::Tuple(items) => {
            items.iter_mut().for_each(|item| match item {
                crate::terlan_typeck::CoreTupleTypeElem::Type(ty)
                | crate::terlan_typeck::CoreTupleTypeElem::Field { ty, .. } => {
                    resolve_constructor_types(ty, constructors)
                }
            });
        }
        CoreType::List(item) => resolve_constructor_types(item, constructors),
        CoreType::Struct { fields, .. } => fields
            .iter_mut()
            .for_each(|field| resolve_constructor_types(&mut field.ty, constructors)),
        CoreType::Map(fields) => fields
            .iter_mut()
            .for_each(|field| resolve_constructor_types(&mut field.value, constructors)),
        CoreType::Arrow {
            params,
            return_type,
        } => {
            params
                .iter_mut()
                .for_each(|param| resolve_constructor_types(param, constructors));
            resolve_constructor_types(return_type, constructors);
        }
        CoreType::Union(types) => types
            .iter_mut()
            .for_each(|ty| resolve_constructor_types(ty, constructors)),
        CoreType::Int
        | CoreType::Float
        | CoreType::Number
        | CoreType::String
        | CoreType::Binary
        | CoreType::Atom
        | CoreType::Bool
        | CoreType::Term
        | CoreType::Dynamic
        | CoreType::Never
        | CoreType::AtomLiteral(_) => {}
    }
}

fn unique_constructor_core_type(
    name: &str,
    constructors: &NativeConstructorLayouts,
) -> Option<CoreType> {
    let mut matches = constructors.iter().filter_map(|((identity, _), layout)| {
        (identity == name || identity.rsplit('.').next() == Some(name))
            .then_some(layout.result_core_type.as_ref())
            .flatten()
    });
    let first = matches.next()?.clone();
    matches
        .all(|candidate| candidate == &first)
        .then_some(first)
}

fn core_type_contains_dynamic(ty: &CoreType) -> bool {
    match ty {
        CoreType::Dynamic => true,
        CoreType::Named(name) if name == "Dynamic" => true,
        CoreType::Apply { args, .. } => args.iter().any(core_type_contains_dynamic),
        CoreType::List(item) => core_type_contains_dynamic(item),
        CoreType::Tuple(items) => items.iter().any(|item| match item {
            crate::terlan_typeck::CoreTupleTypeElem::Type(ty)
            | crate::terlan_typeck::CoreTupleTypeElem::Field { ty, .. } => {
                core_type_contains_dynamic(ty)
            }
        }),
        CoreType::Struct { fields, .. } => fields
            .iter()
            .any(|field| core_type_contains_dynamic(&field.ty)),
        CoreType::Map(fields) => fields
            .iter()
            .any(|field| core_type_contains_dynamic(&field.value)),
        CoreType::Arrow {
            params,
            return_type,
        } => {
            params.iter().any(core_type_contains_dynamic) || core_type_contains_dynamic(return_type)
        }
        CoreType::Union(types) => types.iter().any(core_type_contains_dynamic),
        CoreType::Int
        | CoreType::Float
        | CoreType::Number
        | CoreType::String
        | CoreType::Binary
        | CoreType::Atom
        | CoreType::Bool
        | CoreType::Term
        | CoreType::Never
        | CoreType::AtomLiteral(_)
        | CoreType::Named(_) => false,
    }
}
