//! Concrete structural-union layouts retained after transparent alias expansion.

use std::sync::Arc;

use crate::runtime::native_image::managed::{encode_aggregate_layout, ManagedAggregateDescriptor};
use crate::terlan_typeck::{CoreTupleTypeElem, CoreType};

use super::{
    canonical_structural_field_name, managed_field_type, native_type,
    structural_constructor_fields, NativeConstructorLayout, NativeConstructorLayouts,
};
use crate::compiler::native_ir::expression::managed_semantic_contract;

/// Installs canonical layouts for concrete Option, Result, and tagged-union types.
///
/// Transparent aliases intentionally erase constructor syntax into tuples. The
/// native pipeline must nevertheless retain the concrete union type so tuple
/// literals, equality, and patterns all materialize the same managed semantic.
pub(in crate::compiler::native_ir) fn install_structural_type_layouts<'a>(
    types: impl IntoIterator<Item = &'a CoreType>,
    layouts: &mut NativeConstructorLayouts,
) -> Result<(), String> {
    for ty in types {
        install_type(ty, layouts)?;
    }
    Ok(())
}

fn install_type(ty: &CoreType, layouts: &mut NativeConstructorLayouts) -> Result<(), String> {
    if let Some(variants) = structural_variants(ty) {
        let canonical = managed_semantic_contract(ty);
        let result = native_type(Some(ty), &ty.contract_text()).ok_or_else(|| {
            format!(
                "error[native_ir.structural_registry_type]: `{}` has no native representation",
                ty.contract_text()
            )
        })?;
        for variant in variants {
            let Some((discriminant, variant_count, fields)) =
                structural_constructor_fields(&variant, ty)
            else {
                continue;
            };
            let parameters = fields
                .iter()
                .map(|(_, field)| native_type(Some(field), &field.contract_text()))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.structural_registry_field]: `{}` has an unsupported field",
                        ty.contract_text()
                    )
                })?;
            let descriptor = Arc::new(
                ManagedAggregateDescriptor::constructor(
                    &canonical,
                    &variant,
                    discriminant,
                    variant_count,
                    fields
                        .iter()
                        .enumerate()
                        .zip(&parameters)
                        .map(|((index, (name, _)), ty)| {
                            managed_field_type(*ty).map(|ty| {
                                (
                                    canonical_structural_field_name(
                                        &canonical,
                                        &variant,
                                        index,
                                        name.as_deref(),
                                    ),
                                    ty,
                                )
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                )
                .map_err(|error| format!("error[native_ir.structural_registry]: {error}"))?,
            );
            let encoded_layout =
                Arc::<[u8]>::from(encode_aggregate_layout(&descriptor).map_err(|error| {
                    format!("error[native_ir.structural_registry_abi]: {error}")
                })?);
            let key = (format!("$structural.{canonical}.{variant}"), fields.len());
            layouts
                .entry(key)
                .or_insert_with(|| NativeConstructorLayout {
                    parameters,
                    parameter_core_types: fields
                        .iter()
                        .map(|(_, field)| Some(field.clone()))
                        .collect(),
                    result,
                    result_core_type: Some(ty.clone()),
                    descriptor,
                    encoded_layout,
                });
        }
    }
    match ty {
        CoreType::Apply { args, .. } | CoreType::Union(args) => {
            for argument in args {
                install_type(argument, layouts)?;
            }
        }
        CoreType::Tuple(elements) => {
            for element in elements {
                install_type(tuple_element_type(element), layouts)?;
            }
        }
        CoreType::List(element) => install_type(element, layouts)?,
        CoreType::Struct { fields, .. } => {
            for field in fields {
                install_type(&field.ty, layouts)?;
            }
        }
        CoreType::Map(fields) => {
            for field in fields {
                install_type(&field.value, layouts)?;
            }
        }
        CoreType::Arrow {
            params,
            return_type,
        } => {
            for parameter in params {
                install_type(parameter, layouts)?;
            }
            install_type(return_type, layouts)?;
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
    Ok(())
}

fn structural_variants(ty: &CoreType) -> Option<Vec<String>> {
    match ty {
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Option") && args.len() == 1 =>
        {
            Some(vec!["None".to_string(), "Some".to_string()])
        }
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Result") && args.len() == 2 =>
        {
            Some(vec!["Ok".to_string(), "Err".to_string()])
        }
        CoreType::Union(variants) => variants.iter().map(structural_variant_name).collect(),
        _ => None,
    }
}

fn structural_variant_name(ty: &CoreType) -> Option<String> {
    let atom = match ty {
        CoreType::AtomLiteral(atom) => atom,
        CoreType::Tuple(elements) => match elements.first()? {
            CoreTupleTypeElem::Type(CoreType::AtomLiteral(atom))
            | CoreTupleTypeElem::Field {
                ty: CoreType::AtomLiteral(atom),
                ..
            } => atom,
            _ => return None,
        },
        _ => return None,
    };
    Some(match atom.as_str() {
        "none" => "None".to_string(),
        "error" => "Err".to_string(),
        value => {
            let mut chars = value.chars();
            chars.next()?.to_uppercase().chain(chars).collect()
        }
    })
}

fn tuple_element_type(element: &CoreTupleTypeElem) -> &CoreType {
    match element {
        CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => ty,
    }
}
