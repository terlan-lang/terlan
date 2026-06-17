use std::collections::HashMap;

use super::{
    apply_subst, ConstructorScheme, FunctionBound, FunctionScheme, MapFieldType, Type, TypeVarId,
};

/// Instantiates a function scheme starting from type variable `0`.
///
/// Inputs:
/// - `scheme`: generic function scheme to copy.
///
/// Output:
/// - Function scheme with deterministically remapped type variables.
///
/// Transformation:
/// - Delegates to `instantiate_function_scheme_from` using the default first
///   fresh variable ID.
pub(super) fn instantiate_function_scheme(scheme: &FunctionScheme) -> FunctionScheme {
    instantiate_function_scheme_from(scheme, 0)
}

/// Instantiates a function scheme with fresh type variable IDs.
///
/// Inputs:
/// - `scheme`: generic function scheme to copy.
/// - `first_var`: first type variable ID available for the copy.
///
/// Output:
/// - Function scheme whose generic variables have been remapped to fresh IDs.
///
/// Transformation:
/// - Walks params, return type, and bounds, replacing every scheme-local type
///   variable with a deterministic fresh variable starting at `first_var`.
pub(super) fn instantiate_function_scheme_from(
    scheme: &FunctionScheme,
    first_var: TypeVarId,
) -> FunctionScheme {
    let mut next_var = first_var;
    let mut map: HashMap<TypeVarId, TypeVarId> = HashMap::new();

    let mut remap = |id: &TypeVarId| -> TypeVarId {
        if let Some(remapped) = map.get(id) {
            *remapped
        } else {
            let remapped = next_var;
            map.insert(*id, remapped);
            next_var += 1;
            remapped
        }
    };

    let params = scheme
        .params
        .iter()
        .map(|param| remap_type(param, &mut remap))
        .collect();
    let ret = remap_type(&scheme.ret, &mut remap);
    let bounds = scheme
        .bounds
        .iter()
        .map(|bound| FunctionBound {
            trait_name: bound.trait_name.clone(),
            trait_args: bound
                .trait_args
                .iter()
                .map(|arg| remap_type(arg, &mut remap))
                .collect(),
        })
        .collect();

    FunctionScheme {
        params,
        ret,
        bounds,
    }
}

/// Instantiates a constructor scheme with fresh type variable IDs.
///
/// Inputs:
/// - `scheme`: generic constructor scheme to copy.
/// - `first_var`: first type variable ID available for the copy.
///
/// Output:
/// - Constructor scheme with remapped fixed params, vararg param, and return
///   type.
///
/// Transformation:
/// - Applies the same deterministic fresh-variable remapping used for function
///   schemes to constructor parameter and return types.
pub(super) fn instantiate_constructor_scheme(
    scheme: &ConstructorScheme,
    first_var: TypeVarId,
) -> ConstructorScheme {
    let mut next_var = first_var;
    let mut map: HashMap<TypeVarId, TypeVarId> = HashMap::new();

    let mut remap = |id: &TypeVarId| -> TypeVarId {
        if let Some(remapped) = map.get(id) {
            *remapped
        } else {
            let remapped = next_var;
            map.insert(*id, remapped);
            next_var += 1;
            remapped
        }
    };

    ConstructorScheme {
        fixed_params: scheme
            .fixed_params
            .iter()
            .map(|param| remap_type(param, &mut remap))
            .collect(),
        min_arity: scheme.min_arity,
        vararg: scheme
            .vararg
            .as_ref()
            .map(|param| remap_type(param, &mut remap)),
        ret: remap_type(&scheme.ret, &mut remap),
    }
}

/// Returns the next safe type variable ID for constructor instantiation.
///
/// Inputs:
/// - `args`: argument types already inferred for the constructor call.
/// - `subst`: active substitution map for the enclosing expression check.
///
/// Output:
/// - One greater than the highest type variable visible in arguments or
///   substitutions, or `0` when none are visible.
///
/// Transformation:
/// - Scans argument types plus substitution keys and values so freshly
///   instantiated constructor generics cannot collide with stale bindings.
pub(super) fn next_constructor_type_var(
    args: &[Type],
    subst: &HashMap<TypeVarId, Type>,
) -> TypeVarId {
    next_type_var(args, subst)
}

/// Returns the next safe type variable ID for function-call instantiation.
///
/// Inputs:
/// - `args`: argument types already inferred for the call.
/// - `subst`: active substitution map for the enclosing expression check.
///
/// Output:
/// - One greater than the highest type variable visible in arguments or
///   substitutions, or `0` when none are visible.
///
/// Transformation:
/// - Scans argument types plus substitution keys and values so freshly
///   instantiated function generics cannot collide with stale bindings from
///   earlier calls in the same typechecking pass.
pub(super) fn next_function_type_var(args: &[Type], subst: &HashMap<TypeVarId, Type>) -> TypeVarId {
    next_type_var(args, subst)
}

/// Returns the highest visible type variable plus one.
///
/// Inputs:
/// - `args`: argument types to scan.
/// - `subst`: active substitution map.
///
/// Output:
/// - Next free type variable ID.
///
/// Transformation:
/// - Considers argument type variables, substitution keys, and substitution
///   value type variables in one max calculation.
fn next_type_var(args: &[Type], subst: &HashMap<TypeVarId, Type>) -> TypeVarId {
    let arg_max = args.iter().filter_map(max_type_var).max();
    let subst_key_max = subst.keys().copied().max();
    let subst_value_max = subst.values().filter_map(max_type_var).max();

    arg_max
        .into_iter()
        .chain(subst_key_max)
        .chain(subst_value_max)
        .max()
        .map(|id| id + 1)
        .unwrap_or(0)
}

/// Returns the maximum type variable used by a type.
///
/// Inputs:
/// - `ty`: type to scan.
///
/// Output:
/// - Highest type variable ID found in `ty`, or `None` when the type contains
///   no variables.
///
/// Transformation:
/// - Recursively descends through collection, named, union, tuple, function,
///   and fixed-array types.
pub(super) fn max_type_var(ty: &Type) -> Option<TypeVarId> {
    match ty {
        Type::Var(id) => Some(*id),
        Type::List(inner) | Type::FixedArray { elem: inner, .. } => max_type_var(inner),
        Type::Tuple(items) | Type::Union(items) => items.iter().filter_map(max_type_var).max(),
        Type::Map(fields) => fields
            .iter()
            .filter_map(|field| max_type_var(&field.value))
            .max(),
        Type::Named { args, .. } => args.iter().filter_map(max_type_var).max(),
        Type::Function { params, ret } => params
            .iter()
            .filter_map(max_type_var)
            .chain(max_type_var(ret))
            .max(),
        _ => None,
    }
}

/// Remaps every type variable in a type through a caller-provided mapper.
///
/// Inputs:
/// - `ty`: type to copy.
/// - `remap`: function mapping old type variable IDs to new IDs.
///
/// Output:
/// - Copied type with remapped variable IDs.
///
/// Transformation:
/// - Recursively preserves non-variable structure and applies `remap` only to
///   `Type::Var` IDs.
pub(super) fn remap_type<F>(ty: &Type, remap: &mut F) -> Type
where
    F: FnMut(&TypeVarId) -> TypeVarId,
{
    match ty {
        Type::Var(id) => Type::Var(remap(id)),
        Type::List(inner) => Type::List(Box::new(remap_type(inner, remap))),
        Type::Tuple(items) => {
            Type::Tuple(items.iter().map(|item| remap_type(item, remap)).collect())
        }
        Type::Union(items) => {
            Type::Union(items.iter().map(|item| remap_type(item, remap)).collect())
        }
        Type::Map(fields) => Type::Map(
            fields
                .iter()
                .map(|field| MapFieldType {
                    key: field.key.clone(),
                    value: remap_type(&field.value, remap),
                    required: field.required,
                })
                .collect(),
        ),
        Type::Named { module, name, args } => Type::Named {
            module: module.clone(),
            name: name.clone(),
            args: args.iter().map(|arg| remap_type(arg, remap)).collect(),
        },
        Type::Function { params, ret } => Type::Function {
            params: params
                .iter()
                .map(|param| remap_type(param, remap))
                .collect(),
            ret: Box::new(remap_type(ret, remap)),
        },
        Type::FixedArray { size, elem } => Type::FixedArray {
            size: *size,
            elem: Box::new(remap_type(elem, remap)),
        },
        other => other.clone(),
    }
}

/// Instantiates a type through an existing substitution map.
///
/// Inputs:
/// - `ty`: type to instantiate.
/// - `subst`: type-variable substitution map.
///
/// Output:
/// - Type after applying the substitution.
///
/// Transformation:
/// - Delegates to the type-system substitution helper so scheme instantiation
///   and constructor inference use the same substitution semantics.
pub(super) fn instantiate_type(ty: &Type, subst: &HashMap<TypeVarId, Type>) -> Type {
    apply_subst(ty, subst)
}
