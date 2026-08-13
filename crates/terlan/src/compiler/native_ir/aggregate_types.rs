//! Canonical managed layouts for structural product types at native boundaries.

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_aggregate_layout, ManagedAggregateDescriptor, ManagedFieldType,
};
use crate::terlan_typeck::{
    CoreExpr, CoreIntrinsicId, CorePrimitiveIntrinsic, CoreTupleTypeElem, CoreType,
};

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

pub(super) fn managed_expression_layouts<'a>(
    expressions: impl IntoIterator<Item = &'a CoreExpr>,
) -> Result<Vec<Arc<[u8]>>, String> {
    let mut layouts = BTreeSet::new();
    for expression in expressions {
        inventory_expr(expression, &mut layouts)?;
    }
    Ok(layouts.into_iter().map(Arc::from).collect())
}

/// Builds the one canonical physical descriptor for `std.core.Memory.Layout`.
pub(super) fn memory_layout_descriptor(
) -> Result<(Arc<ManagedAggregateDescriptor>, Arc<[u8]>), String> {
    let descriptor = Arc::new(
        ManagedAggregateDescriptor::record(
            "std.core.Memory.Layout",
            vec![
                ("size".to_string(), ManagedFieldType::Int),
                ("alignment".to_string(), ManagedFieldType::Int),
                ("storage".to_string(), ManagedFieldType::Atom),
            ],
        )
        .map_err(|error| format!("error[native_ir.memory_layout]: {error}"))?,
    );
    let encoded_layout = Arc::from(
        encode_aggregate_layout(&descriptor)
            .map_err(|error| format!("error[native_ir.memory_layout]: {error}"))?,
    );
    Ok((descriptor, encoded_layout))
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
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Option") && args.len() == 1 =>
        {
            let field_type = native_type(Some(&args[0]), &args[0].contract_text())
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.option_layout_type]: unsupported Option field `{}`",
                        args[0].contract_text()
                    )
                })
                .and_then(managed_field_type)?;
            for descriptor in [
                ManagedAggregateDescriptor::constructor(
                    &ty.contract_text(),
                    "None",
                    0,
                    2,
                    Vec::new(),
                ),
                ManagedAggregateDescriptor::constructor(
                    &ty.contract_text(),
                    "Some",
                    1,
                    2,
                    vec![(Some("value".to_string()), field_type)],
                ),
            ] {
                let descriptor = descriptor
                    .map_err(|error| format!("error[native_ir.option_layout]: {error}"))?;
                layouts.insert(
                    encode_aggregate_layout(&descriptor)
                        .map_err(|error| format!("error[native_ir.option_layout_abi]: {error}"))?,
                );
            }
            inventory(&args[0], layouts)?;
        }
        CoreType::Apply { constructor, args }
            if constructor.rsplit('.').next() == Some("Result") && args.len() == 2 =>
        {
            let fields = [("Ok", "value", &args[0]), ("Err", "reason", &args[1])];
            for (discriminant, (variant, field_name, field)) in fields.into_iter().enumerate() {
                let field_type = native_type(Some(field), &field.contract_text())
                    .ok_or_else(|| {
                        format!(
                            "error[native_ir.result_layout_type]: unsupported Result field `{}`",
                            field.contract_text()
                        )
                    })
                    .and_then(managed_field_type)?;
                let descriptor = ManagedAggregateDescriptor::constructor(
                    &ty.contract_text(),
                    variant,
                    u32::try_from(discriminant).map_err(|_| {
                        "error[native_ir.result_layout]: discriminant exceeds u32".to_string()
                    })?,
                    2,
                    vec![(Some(field_name.to_string()), field_type)],
                )
                .map_err(|error| format!("error[native_ir.result_layout]: {error}"))?;
                layouts.insert(
                    encode_aggregate_layout(&descriptor)
                        .map_err(|error| format!("error[native_ir.result_layout_abi]: {error}"))?,
                );
                inventory(field, layouts)?;
            }
        }
        CoreType::Union(args) if tagged_union_variants(args).is_some() => {
            let variants = tagged_union_variants(args).expect("tagged union checked above");
            for (discriminant, (name, fields)) in variants.iter().enumerate() {
                let descriptor = ManagedAggregateDescriptor::constructor(
                    &ty.contract_text(),
                    name,
                    u32::try_from(discriminant).map_err(|_| {
                        "error[native_ir.union_layout]: discriminant exceeds u32".to_string()
                    })?,
                    u32::try_from(variants.len()).map_err(|_| {
                        "error[native_ir.union_layout]: variant count exceeds u32".to_string()
                    })?,
                    fields
                        .iter()
                        .map(|(name, field)| {
                            native_type(Some(field), &field.contract_text())
                                .ok_or_else(|| {
                                    format!(
                                        "error[native_ir.union_layout_type]: unsupported field `{}`",
                                        field.contract_text()
                                    )
                                })
                                .and_then(managed_field_type)
                                .map(|ty| (name.clone(), ty))
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                )
                .map_err(|error| format!("error[native_ir.union_layout]: {error}"))?;
                layouts.insert(
                    encode_aggregate_layout(&descriptor)
                        .map_err(|error| format!("error[native_ir.union_layout_abi]: {error}"))?,
                );
                for (_, field) in fields {
                    inventory(field, layouts)?;
                }
            }
        }
        CoreType::Apply { args, .. } | CoreType::Union(args) => {
            for argument in args {
                inventory(argument, layouts)?;
            }
        }
        CoreType::Struct { name, fields } => {
            let descriptor = ManagedAggregateDescriptor::record(
                name,
                fields
                    .iter()
                    .map(|field| {
                        native_type(Some(&field.ty), &field.ty.contract_text())
                            .ok_or_else(|| {
                                format!(
                                    "error[native_ir.struct_layout_type]: unsupported field `{}`",
                                    field.ty.contract_text()
                                )
                            })
                            .and_then(managed_field_type)
                            .map(|ty| (field.name.clone(), ty))
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            )
            .map_err(|error| format!("error[native_ir.struct_layout]: {error}"))?;
            layouts.insert(
                encode_aggregate_layout(&descriptor)
                    .map_err(|error| format!("error[native_ir.struct_layout_abi]: {error}"))?,
            );
            for field in fields {
                inventory(&field.ty, layouts)?;
            }
        }
        CoreType::Map(fields) => {
            let descriptor = ManagedAggregateDescriptor::record(
                &ty.contract_text(),
                fields
                    .iter()
                    .map(|field| {
                        native_type(Some(&field.value), &field.value.contract_text())
                            .ok_or_else(|| {
                                format!(
                                    "error[native_ir.map_record_layout_type]: unsupported field `{}`",
                                    field.value.contract_text()
                                )
                            })
                            .and_then(managed_field_type)
                            .map(|ty| (field.key.clone(), ty))
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            )
            .map_err(|error| format!("error[native_ir.map_record_layout]: {error}"))?;
            layouts.insert(
                encode_aggregate_layout(&descriptor)
                    .map_err(|error| format!("error[native_ir.map_record_layout_abi]: {error}"))?,
            );
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
        CoreType::Named(name)
            if matches!(
                name.as_str(),
                "Error" | "std.core.Error" | "std.core.Error.Error"
            ) =>
        {
            let descriptor = ManagedAggregateDescriptor::record(
                &ty.contract_text(),
                vec![
                    ("code".to_string(), ManagedFieldType::Atom),
                    (
                        "message".to_string(),
                        ManagedFieldType::Reference(
                            crate::runtime::native_image::managed::managed_string_semantic_id(),
                        ),
                    ),
                ],
            )
            .map_err(|error| format!("error[native_ir.portable_error_layout]: {error}"))?;
            layouts.insert(
                encode_aggregate_layout(&descriptor).map_err(|error| {
                    format!("error[native_ir.portable_error_layout_abi]: {error}")
                })?,
            );
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

type TaggedVariant = (String, Vec<(Option<String>, CoreType)>);

fn tagged_union_variants(types: &[CoreType]) -> Option<Vec<TaggedVariant>> {
    types
        .iter()
        .map(|ty| {
            let (atom, fields) = match ty {
                CoreType::AtomLiteral(atom) => (atom, &[][..]),
                CoreType::Tuple(elements) => {
                    let (first, fields) = elements.split_first()?;
                    let atom = match first {
                        CoreTupleTypeElem::Type(CoreType::AtomLiteral(atom))
                        | CoreTupleTypeElem::Field {
                            ty: CoreType::AtomLiteral(atom),
                            ..
                        } => atom,
                        _ => return None,
                    };
                    (atom, fields)
                }
                _ => return None,
            };
            let name = match atom.as_str() {
                "error" => "Err".to_string(),
                value => {
                    let mut chars = value.chars();
                    chars.next()?.to_uppercase().chain(chars).collect()
                }
            };
            let fields = fields
                .iter()
                .map(|field| match field {
                    CoreTupleTypeElem::Type(ty) => (None, ty.clone()),
                    CoreTupleTypeElem::Field { name, ty } => (Some(name.clone()), ty.clone()),
                })
                .collect();
            Some((name, fields))
        })
        .collect()
}

fn inventory_expr(expr: &CoreExpr, layouts: &mut BTreeSet<Vec<u8>>) -> Result<(), String> {
    match expr {
        CoreExpr::Intrinsic(call) => {
            inventory(&call.return_type, layouts)?;
            if matches!(call.id, CoreIntrinsicId::MemoryLayoutOf(_)) {
                let (_, encoded) = memory_layout_descriptor()?;
                layouts.insert(encoded.as_ref().to_vec());
            }
            if matches!(
                call.id,
                CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapFromEntries)
            ) {
                inventory_map_entry_pair(&call.return_type, layouts)?;
            }
            inventory_exprs(&call.args, layouts)
        }
        CoreExpr::Cast { expr, target_type } => {
            inventory(target_type, layouts)?;
            inventory_expr(expr, layouts)
        }
        CoreExpr::Tuple(items)
        | CoreExpr::List(items)
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
        CoreExpr::SqlQuery {
            parameters,
            result_core_type,
            ..
        } => {
            inventory(result_core_type, layouts)?;
            inventory_exprs(parameters, layouts)
        }
        CoreExpr::Case { scrutinee, clauses } => {
            inventory_expr(scrutinee, layouts)?;
            inventory_clauses(clauses, layouts)
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            inventory_expr(body, layouts)?;
            inventory_clauses(of_clauses, layouts)?;
            inventory_clauses(catch_clauses, layouts)?;
            if let Some(after) = after_clause {
                inventory_expr(&after.trigger, layouts)?;
                inventory_expr(&after.body, layouts)?;
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

fn inventory_map_entry_pair(map: &CoreType, layouts: &mut BTreeSet<Vec<u8>>) -> Result<(), String> {
    let CoreType::Apply { constructor, args } = map else {
        return Ok(());
    };
    if constructor.rsplit('.').next() != Some("Map") || args.len() != 2 {
        return Ok(());
    }
    inventory(
        &CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(args[0].clone()),
            CoreTupleTypeElem::Type(args[1].clone()),
        ]),
        layouts,
    )
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

fn inventory_clauses(
    clauses: &[crate::terlan_typeck::CoreCaseClause],
    layouts: &mut BTreeSet<Vec<u8>>,
) -> Result<(), String> {
    for clause in clauses {
        if let Some(guard) = &clause.guard {
            inventory_expr(guard, layouts)?;
        }
        inventory_expr(&clause.body, layouts)?;
    }
    Ok(())
}
