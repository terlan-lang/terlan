//! Checked collection-schema inventory for direct AOT images.

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_collection_layout, ManagedCollectionDescriptor, ManagedFieldType,
};
use crate::terlan_typeck::{
    CoreExpr, CoreIntrinsicId, CorePrimitiveIntrinsic, CoreTupleTypeElem, CoreType,
};

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

/// Encodes collection schemas introduced by typed expression results and operands.
pub(super) fn managed_expression_collection_layouts<'a>(
    expressions: impl IntoIterator<Item = &'a CoreExpr>,
) -> Result<Vec<Arc<[u8]>>, String> {
    let mut layouts = BTreeSet::new();
    for expression in expressions {
        inventory_expr(expression, &mut layouts)?;
    }
    Ok(layouts.into_iter().map(Arc::from).collect())
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

fn inventory_expr(expr: &CoreExpr, layouts: &mut BTreeSet<Vec<u8>>) -> Result<(), String> {
    match expr {
        CoreExpr::Intrinsic(call) => {
            inventory_type(&call.return_type, layouts)?;
            if matches!(
                call.id,
                CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapFromEntries)
            ) {
                inventory_map_entry_list(&call.return_type, layouts)?;
            }
            if matches!(
                call.id,
                CoreIntrinsicId::Primitive(
                    CorePrimitiveIntrinsic::ListIterator
                        | CorePrimitiveIntrinsic::MapIterator
                        | CorePrimitiveIntrinsic::SetIterator
                )
            ) {
                inventory_iterator_storage(&call.return_type, layouts)?;
            }
            inventory_exprs(&call.args, layouts)
        }
        CoreExpr::Cast { expr, target_type } => {
            inventory_type(target_type, layouts)?;
            inventory_expr(expr, layouts)
        }
        CoreExpr::List(items) => {
            if let Some(ty) = super::expression::literal_collection_type(expr)
                .or_else(|| super::expression::witnessed_collection_type(expr))
            {
                inventory_type(&ty, layouts)?;
            }
            inventory_exprs(items, layouts)
        }
        CoreExpr::Tuple(items)
        | CoreExpr::FixedArray(items)
        | CoreExpr::RemoteCall { args: items, .. }
        | CoreExpr::ConstructorCall { args: items, .. }
        | CoreExpr::Call { args: items, .. } => inventory_exprs(items, layouts),
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        }
        | CoreExpr::BinaryOp {
            left: head,
            right: tail,
            ..
        } => {
            inventory_expr(head, layouts)?;
            inventory_expr(tail, layouts)
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                inventory_expr(&binding.value, layouts)?;
            }
            inventory_expr(body, layouts)
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                inventory_expr(&field.value, layouts)?;
            }
            Ok(())
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                inventory_expr(&field.value, layouts)?;
            }
            Ok(())
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            inventory_expr(base, layouts)?;
            for field in fields {
                inventory_expr(&field.value, layouts)?;
            }
            Ok(())
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            inventory_expr(base, layouts)
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            inventory_exprs(args, layouts)?;
            inventory_expr(record, layouts)
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            inventory_expr(receiver, layouts)?;
            inventory_exprs(args, layouts)
        }
        CoreExpr::FunctionCall { callee, args } => {
            inventory_expr(callee, layouts)?;
            inventory_exprs(args, layouts)
        }
        CoreExpr::SqlQuery { parameters, .. } => inventory_exprs(parameters, layouts),
        CoreExpr::Case { scrutinee, clauses } => {
            inventory_expr(scrutinee, layouts)?;
            for clause in clauses {
                if let Some(guard) = &clause.guard {
                    inventory_expr(guard, layouts)?;
                }
                inventory_expr(&clause.body, layouts)?;
            }
            Ok(())
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                inventory_expr(&clause.condition, layouts)?;
                inventory_expr(&clause.body, layouts)?;
            }
            Ok(())
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            inventory_expr(body, layouts)?;
            for clause in of_clauses.iter().chain(catch_clauses) {
                if let Some(guard) = &clause.guard {
                    inventory_expr(guard, layouts)?;
                }
                inventory_expr(&clause.body, layouts)?;
            }
            if let Some(after) = after_clause {
                inventory_expr(&after.trigger, layouts)?;
                inventory_expr(&after.body, layouts)?;
            }
            Ok(())
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            inventory_expr(expr, layouts)?;
            for generator in generators {
                inventory_expr(&generator.source, layouts)?;
            }
            inventory_exprs(guards, layouts)
        }
        CoreExpr::Lam { body, .. } | CoreExpr::UnaryOp { operand: body, .. } => {
            inventory_expr(body, layouts)
        }
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => Ok(()),
    }
}

fn inventory_exprs(
    expressions: &[CoreExpr],
    layouts: &mut BTreeSet<Vec<u8>>,
) -> Result<(), String> {
    for expression in expressions {
        inventory_expr(expression, layouts)?;
    }
    Ok(())
}

fn inventory_map_entry_list(map: &CoreType, layouts: &mut BTreeSet<Vec<u8>>) -> Result<(), String> {
    let CoreType::Apply { constructor, args } = map else {
        return Ok(());
    };
    if constructor.rsplit('.').next() != Some("Map") || args.len() != 2 {
        return Ok(());
    }
    let pair = CoreType::Tuple(vec![
        CoreTupleTypeElem::Type(args[0].clone()),
        CoreTupleTypeElem::Type(args[1].clone()),
    ]);
    inventory_type(&CoreType::List(Box::new(pair)), layouts)
}

/// Iterators use immutable managed lists as their direct-AOT physical storage.
/// The public opaque `Iterator[T]` type alone does not inventory that backing
/// collection, so every iterator intrinsic must admit `List[T]` explicitly.
fn inventory_iterator_storage(
    iterator: &CoreType,
    layouts: &mut BTreeSet<Vec<u8>>,
) -> Result<(), String> {
    let CoreType::Apply { constructor, args } = iterator else {
        return Ok(());
    };
    if constructor.rsplit('.').next() != Some("Iterator") || args.len() != 1 {
        return Ok(());
    }
    inventory_type(&CoreType::List(Box::new(args[0].clone())), layouts)
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
#[cfg(test)]
mod collections_test;
