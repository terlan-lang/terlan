//! Canonical managed layouts for structural product types at native boundaries.

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::runtime::native_image::managed::{encode_aggregate_layout, ManagedAggregateDescriptor};
use crate::terlan_typeck::{CoreTupleTypeElem, CoreType};

use super::{constructors::managed_field_type, native_type};

pub(super) fn managed_aggregate_layouts<'a>(
    types: impl IntoIterator<Item = &'a CoreType>,
) -> Result<Vec<Arc<[u8]>>, String> {
    let mut layouts = BTreeSet::new();
    for ty in types {
        inventory(ty, &mut layouts)?;
    }
    Ok(layouts.into_iter().map(Arc::from).collect())
}

fn inventory(ty: &CoreType, layouts: &mut BTreeSet<Vec<u8>>) -> Result<(), String> {
    match ty {
        CoreType::Tuple(elements) => {
            let fields = elements
                .iter()
                .map(|element| match element {
                    CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => ty,
                })
                .map(|ty| {
                    native_type(Some(ty), &ty.contract_text())
                        .ok_or_else(|| {
                            format!(
                                "error[native_ir.tuple_layout_type]: unsupported tuple field `{}`",
                                ty.contract_text()
                            )
                        })
                        .and_then(managed_field_type)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let descriptor = ManagedAggregateDescriptor::tuple(&ty.contract_text(), fields)
                .map_err(|error| format!("error[native_ir.tuple_layout]: {error}"))?;
            layouts.insert(
                encode_aggregate_layout(&descriptor)
                    .map_err(|error| format!("error[native_ir.tuple_layout_abi]: {error}"))?,
            );
            for element in elements {
                inventory(
                    match element {
                        CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => ty,
                    },
                    layouts,
                )?;
            }
        }
        CoreType::List(element) => inventory(element, layouts)?,
        CoreType::Apply { args, .. } | CoreType::Union(args) => {
            for argument in args {
                inventory(argument, layouts)?;
            }
        }
        CoreType::Struct { fields, .. } => {
            for field in fields {
                inventory(&field.ty, layouts)?;
            }
        }
        CoreType::Map(fields) => {
            for field in fields {
                inventory(&field.value, layouts)?;
            }
        }
        CoreType::Arrow {
            params,
            return_type,
        } => {
            for parameter in params {
                inventory(parameter, layouts)?;
            }
            inventory(return_type, layouts)?;
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
