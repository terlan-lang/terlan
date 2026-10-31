//! Nominal declaration parameters inherited by specialized receiver methods.

use std::collections::{HashMap, HashSet};

use crate::terlan_typeck::{CoreFunction, CoreModule, CoreTupleTypeElem, CoreType};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GenericConformance {
    pub(super) instance: CoreType,
    pub(super) generic_params: Vec<String>,
}

pub(super) fn collect_conformances(
    cores: &[CoreModule],
) -> HashMap<String, Vec<GenericConformance>> {
    let nominal_parameters = cores
        .iter()
        .flat_map(|core| {
            core.types.iter().flat_map(|declaration| {
                let parameters = declaration.params.clone();
                [
                    (
                        format!("{}.{}", core.module, declaration.name),
                        parameters.clone(),
                    ),
                    (declaration.name.clone(), parameters),
                ]
            })
        })
        .collect::<HashMap<_, _>>();
    let mut catalog = HashMap::<String, Vec<GenericConformance>>::new();
    for core in cores {
        let local_types = core
            .types
            .iter()
            .map(|declaration| declaration.name.as_str())
            .collect::<HashSet<_>>();
        for conformance in &core.trait_conformances {
            let Some(mut instance) =
                crate::terlan_typeck::core_type_from_text(&conformance.trait_ref)
            else {
                continue;
            };
            instance.visit_names_mut(&mut |name| {
                if !name.contains('.') && local_types.contains(name.as_str()) {
                    *name = format!("{}.{}", core.module, name);
                }
            });
            let CoreType::Apply { constructor, .. } = &instance else {
                continue;
            };
            let generic_params = conformance_generic_params(
                &conformance.for_type,
                &core.module,
                &local_types,
                &nominal_parameters,
            );
            let candidate = GenericConformance {
                instance: instance.clone(),
                generic_params,
            };
            for key in [
                constructor.clone(),
                constructor
                    .rsplit('.')
                    .next()
                    .unwrap_or(constructor)
                    .to_string(),
            ] {
                let candidates = catalog.entry(key).or_default();
                if !candidates.contains(&candidate) {
                    candidates.push(candidate.clone());
                }
            }
        }
    }
    catalog
}

/// Declaration parameters keyed by both public and internal nominal identity.
///
/// Source backends use the same catalog as native specialization so an
/// implicit generic export cannot be removed by one phase and retained as an
/// executable root by another.
pub(in crate::compiler::native_ir) fn nominal_parameter_catalog(
    cores: &[CoreModule],
) -> HashMap<String, Vec<String>> {
    cores
        .iter()
        .flat_map(|core| {
            core.types
                .iter()
                .filter(|declaration| !declaration.params.is_empty())
                .flat_map(|declaration| {
                    let parameters = declaration.params.clone();
                    [
                        (
                            crate::terlan_hir::public_type_path(&core.module, &declaration.name),
                            parameters.clone(),
                        ),
                        (format!("{}.{}", core.module, declaration.name), parameters),
                    ]
                })
        })
        .collect()
}

fn conformance_generic_params(
    for_type: &str,
    module: &str,
    local_types: &HashSet<&str>,
    nominal_parameters: &HashMap<String, Vec<String>>,
) -> Vec<String> {
    let Some(mut owner) = crate::terlan_typeck::core_type_from_text(for_type) else {
        return Vec::new();
    };
    owner.visit_names_mut(&mut |name| {
        if !name.contains('.') && local_types.contains(name.as_str()) {
            *name = format!("{module}.{name}");
        }
    });
    match owner {
        CoreType::Apply { constructor, args } => nominal_parameters
            .get(&constructor)
            .or_else(|| {
                nominal_parameters.get(constructor.rsplit('.').next().unwrap_or(&constructor))
            })
            .into_iter()
            .flatten()
            .zip(args)
            .filter_map(|(parameter, argument)| match argument {
                CoreType::Named(name) if &name == parameter => Some(name),
                _ => None,
            })
            .collect(),
        CoreType::Named(name) if name.len() == 1 && name.as_bytes()[0].is_ascii_uppercase() => {
            vec![name]
        }
        _ => Vec::new(),
    }
}

/// Canonical declaration order shared by contextual inference and instantiation.
pub(in crate::compiler::native_ir) fn generic_parameters(function: &CoreFunction) -> Vec<String> {
    if !function.generic_params.is_empty() {
        return function
            .generic_params
            .iter()
            .map(|parameter| {
                parameter
                    .split_once("=>")
                    .map_or(parameter.as_str(), |(name, _)| name)
                    .trim()
                    .trim_start_matches(['-', '+'])
                    .to_string()
            })
            .collect();
    }
    let mut names = HashSet::new();
    for ty in function
        .params
        .iter()
        .filter_map(|parameter| parameter.core_ty.as_ref())
        .chain(function.core_return_type.as_ref())
    {
        collect_implicit_generic_params(ty, &mut names);
    }
    let mut names = names.into_iter().collect::<Vec<_>>();
    names.sort();
    names
}

/// Recovers implicit receiver generics inherited from a nominal declaration.
///
/// Interface receiver methods spell these variables in the receiver type but
/// do not repeat a method-level generic parameter list. Long names such as
/// `ModelState` are semantic parameters when they occupy the corresponding
/// slots of `ModelTransition[ModelState, ModelObservation]`; they are not
/// ordinary nominal types and must participate in monomorphization.
pub(in crate::compiler::native_ir) fn generic_parameters_with_nominals(
    function: &CoreFunction,
    nominal_parameters: &HashMap<String, Vec<String>>,
) -> Vec<String> {
    let mut parameters = generic_parameters(function);
    let mut inherited = Vec::new();
    for ty in function
        .params
        .iter()
        .filter_map(|parameter| parameter.core_ty.as_ref())
        .chain(function.core_return_type.as_ref())
    {
        collect_nominal_parameters(ty, nominal_parameters, &mut inherited);
    }
    for parameter in inherited {
        if !parameters.contains(&parameter) {
            parameters.push(parameter);
        }
    }
    parameters
}

fn collect_nominal_parameters(
    ty: &CoreType,
    nominal_parameters: &HashMap<String, Vec<String>>,
    inherited: &mut Vec<String>,
) {
    match ty {
        CoreType::Apply { constructor, args } => {
            if let Some(parameters) = nominal_parameters.get(constructor) {
                for (parameter, argument) in parameters.iter().zip(args) {
                    if argument == &CoreType::Named(parameter.clone())
                        && !inherited.contains(parameter)
                    {
                        inherited.push(parameter.clone());
                    }
                }
            }
            for argument in args {
                collect_nominal_parameters(argument, nominal_parameters, inherited);
            }
        }
        CoreType::Union(items) => {
            for item in items {
                collect_nominal_parameters(item, nominal_parameters, inherited);
            }
        }
        CoreType::Tuple(elements) => {
            for element in elements {
                let (CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. }) = element;
                collect_nominal_parameters(ty, nominal_parameters, inherited);
            }
        }
        CoreType::List(element) => {
            collect_nominal_parameters(element, nominal_parameters, inherited)
        }
        CoreType::Struct { fields, .. } => {
            for field in fields {
                collect_nominal_parameters(&field.ty, nominal_parameters, inherited);
            }
        }
        CoreType::Map(fields) => {
            for field in fields {
                collect_nominal_parameters(&field.value, nominal_parameters, inherited);
            }
        }
        CoreType::Arrow {
            params,
            return_type,
        } => {
            for parameter in params {
                collect_nominal_parameters(parameter, nominal_parameters, inherited);
            }
            collect_nominal_parameters(return_type, nominal_parameters, inherited);
        }
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
        | CoreType::AtomLiteral(_)
        | CoreType::Named(_) => {}
    }
}

pub(in crate::compiler::native_ir) fn collect_implicit_generic_params(
    ty: &CoreType,
    names: &mut HashSet<String>,
) {
    match ty {
        CoreType::Named(name) if name.len() == 1 && name.as_bytes()[0].is_ascii_uppercase() => {
            names.insert(name.clone());
        }
        CoreType::Apply { args, .. } | CoreType::Union(args) => {
            for ty in args {
                collect_implicit_generic_params(ty, names);
            }
        }
        CoreType::Tuple(elements) => {
            for element in elements {
                match element {
                    CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => {
                        collect_implicit_generic_params(ty, names)
                    }
                }
            }
        }
        CoreType::List(element) => collect_implicit_generic_params(element, names),
        CoreType::Struct { fields, .. } => {
            for field in fields {
                collect_implicit_generic_params(&field.ty, names);
            }
        }
        CoreType::Map(fields) => {
            for field in fields {
                collect_implicit_generic_params(&field.value, names);
            }
        }
        CoreType::Arrow {
            params,
            return_type,
        } => {
            for ty in params {
                collect_implicit_generic_params(ty, names);
            }
            collect_implicit_generic_params(return_type, names);
        }
        _ => {}
    }
}

pub(in crate::compiler::native_ir) fn contains_generic_parameter(
    ty: &CoreType,
    parameters: &[String],
) -> bool {
    match ty {
        CoreType::Named(name) => parameters.contains(name),
        CoreType::Apply { args, .. } | CoreType::Union(args) => args
            .iter()
            .any(|ty| contains_generic_parameter(ty, parameters)),
        CoreType::List(element) => contains_generic_parameter(element, parameters),
        CoreType::Tuple(elements) => elements.iter().any(|element| match element {
            CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => {
                contains_generic_parameter(ty, parameters)
            }
        }),
        CoreType::Struct { fields, .. } => fields
            .iter()
            .any(|field| contains_generic_parameter(&field.ty, parameters)),
        CoreType::Map(fields) => fields
            .iter()
            .any(|field| contains_generic_parameter(&field.value, parameters)),
        CoreType::Arrow {
            params,
            return_type,
        } => {
            params
                .iter()
                .any(|ty| contains_generic_parameter(ty, parameters))
                || contains_generic_parameter(return_type, parameters)
        }
        _ => false,
    }
}
