//! Checked collection-schema inventory for direct AOT images.

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_collection_layout, ManagedCollectionDescriptor, ManagedFieldType,
};
use crate::terlan_typeck::{CoreTupleTypeElem, CoreType};

use super::{constructors::managed_field_type, native_type};

/// Encodes every concrete List, Map, and Set schema reachable from checked types.
pub(super) fn managed_collection_layouts<'a>(
    types: impl IntoIterator<Item = &'a CoreType>,
) -> Result<Vec<Arc<[u8]>>, String> {
    let mut layouts = BTreeSet::new();
    for ty in types {
        inventory_type(ty, &mut layouts)?;
    }
    Ok(layouts
        .into_iter()
        .map(Arc::<[u8]>::from)
        .collect::<Vec<_>>())
}

/// Inventories one type and every nested collection argument exactly once.
fn inventory_type(ty: &CoreType, layouts: &mut BTreeSet<Vec<u8>>) -> Result<(), String> {
    match ty {
        CoreType::List(element) => {
            insert_list(ty, element, layouts)?;
            inventory_type(element, layouts)?;
        }
        CoreType::Apply { constructor, args } => {
            match constructor.rsplit('.').next() {
                Some("List") if args.len() == 1 => insert_list(ty, &args[0], layouts)?,
                Some("Map") if args.len() == 2 => insert_map(ty, &args[0], &args[1], layouts)?,
                Some("Set") if args.len() == 1 => insert_set(ty, &args[0], layouts)?,
                Some("List" | "Map" | "Set") => {
                    return Err(format!(
                        "error[native_ir.collection_arity]: `{}` has an invalid collection arity",
                        ty.contract_text()
                    ));
                }
                _ => {}
            }
            for argument in args {
                inventory_type(argument, layouts)?;
            }
        }
        CoreType::Tuple(elements) => {
            for element in elements {
                match element {
                    CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => {
                        inventory_type(ty, layouts)?
                    }
                }
            }
        }
        CoreType::Struct { fields, .. } => {
            for field in fields {
                inventory_type(&field.ty, layouts)?;
            }
        }
        CoreType::Map(fields) => {
            for field in fields {
                inventory_type(&field.value, layouts)?;
            }
        }
        CoreType::Arrow {
            params,
            return_type,
        } => {
            for parameter in params {
                inventory_type(parameter, layouts)?;
            }
            inventory_type(return_type, layouts)?;
        }
        CoreType::Union(items) => {
            for item in items {
                inventory_type(item, layouts)?;
            }
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

/// Inserts one canonical list schema.
fn insert_list(
    collection: &CoreType,
    element: &CoreType,
    layouts: &mut BTreeSet<Vec<u8>>,
) -> Result<(), String> {
    let descriptor = ManagedCollectionDescriptor::list(
        &collection.contract_text(),
        collection_field_type(element)?,
    )
    .map_err(collection_schema_error)?;
    layouts.insert(encode_collection_layout(&descriptor).map_err(collection_schema_error)?);
    Ok(())
}

/// Inserts one canonical map schema.
fn insert_map(
    collection: &CoreType,
    key: &CoreType,
    value: &CoreType,
    layouts: &mut BTreeSet<Vec<u8>>,
) -> Result<(), String> {
    let descriptor = ManagedCollectionDescriptor::map(
        &collection.contract_text(),
        collection_field_type(key)?,
        collection_field_type(value)?,
    )
    .map_err(collection_schema_error)?;
    layouts.insert(encode_collection_layout(&descriptor).map_err(collection_schema_error)?);
    Ok(())
}

/// Inserts one canonical set schema.
fn insert_set(
    collection: &CoreType,
    element: &CoreType,
    layouts: &mut BTreeSet<Vec<u8>>,
) -> Result<(), String> {
    let descriptor = ManagedCollectionDescriptor::set(
        &collection.contract_text(),
        collection_field_type(element)?,
    )
    .map_err(collection_schema_error)?;
    layouts.insert(encode_collection_layout(&descriptor).map_err(collection_schema_error)?);
    Ok(())
}

/// Converts one concrete collection slot through the canonical NativeIR type map.
fn collection_field_type(ty: &CoreType) -> Result<ManagedFieldType, String> {
    native_type(Some(ty), &ty.contract_text())
        .ok_or_else(|| {
            format!(
                "error[native_ir.collection_type]: `{}` is not a concrete managed collection field",
                ty.contract_text()
            )
        })
        .and_then(managed_field_type)
}

/// Adds compiler ownership to one managed-schema validation failure.
fn collection_schema_error(error: impl std::fmt::Display) -> String {
    format!("error[native_ir.collection_schema]: {error}")
}

#[cfg(test)]
#[path = "collections_test.rs"]
mod collections_test;
