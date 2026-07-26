//! Native-package declarations, aliases, and opaque handle layouts.

use std::collections::{HashMap, HashSet};

use crate::terlan_syntax::span::Span;
use crate::terlan_typeck::{
    core_type_from_text, CoreEffectSet, CoreExpr, CoreIntrinsicCall, CoreIntrinsicId, CoreModule,
    CoreStructTypeField, CoreType,
};

/// Replaces declarations without Terlan bodies with explicit suspending
/// package-capability wrappers before application admission.
pub(super) fn lower_compiler_native_declarations(
    core: &mut CoreModule,
    aliases: &HashMap<String, (String, CoreType)>,
) -> Result<(), String> {
    for function in &mut core.functions {
        let Some(operation) = function.native_operation.take() else {
            continue;
        };
        let parameter_types = function
            .params
            .iter_mut()
            .map(|parameter| {
                let ty = parameter
                    .core_ty
                    .clone()
                    .or_else(|| core_type_from_text(&parameter.ty))
                    .ok_or_else(|| {
                        format!(
                            "error[native_ir.native_operation_parameter]: `{}` has no checked type",
                            parameter.name
                        )
                    })?;
                let ty = resolve_native_package_type(&ty, aliases, None, &mut HashSet::new());
                parameter.core_ty = Some(ty.clone());
                Ok::<CoreType, String>(ty)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let return_type = function
            .core_return_type
            .clone()
            .or_else(|| core_type_from_text(&function.return_type))
            .ok_or_else(|| {
                format!(
                    "error[native_ir.native_operation_result]: `{operation}` has no checked result type"
                )
            })?;
        let return_type =
            resolve_native_package_type(&return_type, aliases, None, &mut HashSet::new());
        function.core_return_type = Some(return_type.clone());
        let body = CoreExpr::Intrinsic(CoreIntrinsicCall {
            id: CoreIntrinsicId::NativeOperation {
                operation,
                parameter_types,
            },
            args: function
                .params
                .iter()
                .map(|parameter| CoreExpr::Var(parameter.name.clone()))
                .collect(),
            return_type,
            effects: CoreEffectSet {
                effects: vec!["native-package".to_string()],
            },
            span: Span::new(0, 0),
        });
        let [clause] = function.clauses.as_mut_slice() else {
            return Err(format!(
                "error[native_ir.native_operation_clause]: `{}` must have one direct clause",
                function.name
            ));
        };
        clause.body.core_expr = Some(body);
    }
    Ok(())
}

/// Inventories aliases with a concrete package-boundary representation.
pub(super) fn native_package_aliases(cores: &[CoreModule]) -> HashMap<String, (String, CoreType)> {
    cores
        .iter()
        .flat_map(|core| {
            core.types.iter().filter_map(move |declaration| {
                let canonical = format!("{}.{}", core.module, declaration.name);
                let body = if matches!(
                    declaration.visibility,
                    crate::terlan_typeck::CoreVisibility::Opaque
                ) {
                    Some(CoreType::Struct {
                        name: canonical.clone(),
                        fields: vec![
                            CoreStructTypeField {
                                name: "$native_owner".to_string(),
                                ty: CoreType::String,
                                is_private: true,
                            },
                            CoreStructTypeField {
                                name: "$native_id".to_string(),
                                ty: CoreType::Int,
                                is_private: true,
                            },
                            CoreStructTypeField {
                                name: "$native_generation".to_string(),
                                ty: CoreType::Int,
                                is_private: true,
                            },
                            CoreStructTypeField {
                                name: "$native_type".to_string(),
                                ty: CoreType::String,
                                is_private: true,
                            },
                        ],
                    })
                } else {
                    declaration.core_body.clone()
                }?;
                Some((canonical, (core.module.clone(), body)))
            })
        })
        .collect()
}

/// Resolves aliases recursively to their physical transition-buffer values.
pub(super) fn resolve_native_package_type(
    ty: &CoreType,
    aliases: &HashMap<String, (String, CoreType)>,
    alias_module: Option<&str>,
    visiting: &mut HashSet<String>,
) -> CoreType {
    match ty {
        CoreType::Named(name) => {
            let key = if aliases.contains_key(name) {
                name.clone()
            } else if let Some(module) = alias_module {
                format!("{module}.{name}")
            } else {
                name.clone()
            };
            let Some((module, alias)) = aliases.get(&key) else {
                return ty.clone();
            };
            if !visiting.insert(key.clone()) {
                return ty.clone();
            }
            let resolved = resolve_native_package_type(alias, aliases, Some(module), visiting);
            visiting.remove(&key);
            resolved
        }
        CoreType::Apply { constructor, args } => CoreType::Apply {
            constructor: constructor.clone(),
            args: args
                .iter()
                .map(|arg| resolve_native_package_type(arg, aliases, alias_module, visiting))
                .collect(),
        },
        CoreType::List(item) => CoreType::List(Box::new(resolve_native_package_type(
            item,
            aliases,
            alias_module,
            visiting,
        ))),
        CoreType::Tuple(items) => CoreType::Tuple(
            items
                .iter()
                .map(|item| match item {
                    crate::terlan_typeck::CoreTupleTypeElem::Type(ty) => {
                        crate::terlan_typeck::CoreTupleTypeElem::Type(resolve_native_package_type(
                            ty,
                            aliases,
                            alias_module,
                            visiting,
                        ))
                    }
                    crate::terlan_typeck::CoreTupleTypeElem::Field { name, ty } => {
                        crate::terlan_typeck::CoreTupleTypeElem::Field {
                            name: name.clone(),
                            ty: resolve_native_package_type(ty, aliases, alias_module, visiting),
                        }
                    }
                })
                .collect(),
        ),
        CoreType::Struct { name, fields } => CoreType::Struct {
            name: name.clone(),
            fields: fields
                .iter()
                .cloned()
                .map(|mut field| {
                    field.ty =
                        resolve_native_package_type(&field.ty, aliases, alias_module, visiting);
                    field
                })
                .collect(),
        },
        CoreType::Map(fields) => CoreType::Map(
            fields
                .iter()
                .cloned()
                .map(|mut field| {
                    field.value =
                        resolve_native_package_type(&field.value, aliases, alias_module, visiting);
                    field
                })
                .collect(),
        ),
        CoreType::Arrow {
            params,
            return_type,
        } => CoreType::Arrow {
            params: params
                .iter()
                .map(|param| resolve_native_package_type(param, aliases, alias_module, visiting))
                .collect(),
            return_type: Box::new(resolve_native_package_type(
                return_type,
                aliases,
                alias_module,
                visiting,
            )),
        },
        CoreType::Union(types) => CoreType::Union(
            types
                .iter()
                .map(|ty| resolve_native_package_type(ty, aliases, alias_module, visiting))
                .collect(),
        ),
        _ => ty.clone(),
    }
}

/// Installs a private physical representation for opaque package resources.
pub(super) fn native_handle_layouts(
    core: &CoreModule,
) -> Result<Vec<std::sync::Arc<[u8]>>, String> {
    use crate::runtime::native_image::managed::{
        encode_aggregate_layout, ManagedAggregateDescriptor, ManagedFieldType, SemanticTypeId,
    };

    let string = SemanticTypeId::from_canonical("std.core.String")
        .map_err(|error| format!("error[native_ir.native_handle_layout]: {error}"))?;
    core.types
        .iter()
        .filter(|declaration| {
            matches!(
                declaration.visibility,
                crate::terlan_typeck::CoreVisibility::Opaque
            )
        })
        .map(|declaration| {
            let canonical = format!("{}.{}", core.module, declaration.name);
            let descriptor = ManagedAggregateDescriptor::record(
                &canonical,
                vec![
                    (
                        "$native_owner".to_string(),
                        ManagedFieldType::Reference(string),
                    ),
                    ("$native_id".to_string(), ManagedFieldType::Int),
                    ("$native_generation".to_string(), ManagedFieldType::Int),
                    (
                        "$native_type".to_string(),
                        ManagedFieldType::Reference(string),
                    ),
                ],
            )
            .map_err(|error| format!("error[native_ir.native_handle_layout]: {error}"))?;
            encode_aggregate_layout(&descriptor)
                .map(std::sync::Arc::from)
                .map_err(|error| format!("error[native_ir.native_handle_layout]: {error}"))
        })
        .collect()
}
