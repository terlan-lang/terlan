//! Native-package declarations, aliases, and opaque handle layouts.

use std::collections::{HashMap, HashSet};

use crate::terlan_syntax::span::Span;
use crate::terlan_typeck::{
    core_type_from_text, CoreEffectSet, CoreExpr, CoreIntrinsicCall, CoreIntrinsicId, CoreModule,
    CoreStructTypeField, CoreType,
};

/// Replaces declarations without Terlan bodies with explicit suspending
/// package-capability wrappers before application admission.
pub(super) fn lower_compiler_native_declarations(core: &mut CoreModule) -> Result<(), String> {
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
                if is_compiler_managed_value_facade(&canonical) {
                    return None;
                }
                let body = if matches!(
                    declaration.visibility,
                    crate::terlan_typeck::CoreVisibility::Opaque
                ) && declaration.core_body.is_none()
                {
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

/// Reports whether a bodyless opaque type has a compiler-owned physical
/// representation rather than a native-package capability handle.
///
/// `Template.Html` is typechecked as an opaque trust boundary, then erased to
/// an owner-local managed string by template lowering. Treating it as a native
/// resource would replace list element types with the four-field capability
/// handle layout before that lowering runs.
fn is_compiler_managed_value_facade(canonical: &str) -> bool {
    matches!(
        canonical,
        "std.template.Template.Html" | "std.http.Response.Response"
    )
}

/// Resolves every application-visible opaque package type to one canonical
/// storage identity before closure conversion can capture it.
pub(super) fn canonicalize_native_package_types(
    cores: &mut [CoreModule],
    aliases: &HashMap<String, (String, CoreType)>,
) -> Result<(), String> {
    let opaque = cores
        .iter()
        .flat_map(|core| {
            core.types
                .iter()
                .filter(|&declaration| {
                    matches!(
                        declaration.visibility,
                        crate::terlan_typeck::CoreVisibility::Opaque
                    )
                })
                .map(|declaration| format!("{}.{}", core.module, declaration.name))
        })
        .filter_map(|canonical| {
            aliases
                .get(&canonical)
                .cloned()
                .map(|alias| (canonical, alias))
        })
        .collect::<HashMap<_, _>>();
    if opaque.is_empty() {
        return Ok(());
    }

    for core in cores {
        let module = core.module.clone();
        let imports = core
            .imports
            .iter()
            .filter(|import| {
                matches!(
                    import.kind,
                    crate::terlan_typeck::CoreImportKind::Module
                        | crate::terlan_typeck::CoreImportKind::TypeModule
                )
            })
            .map(|import| import.module.clone())
            .collect::<Vec<_>>();
        let canonicalize = |ty: &mut CoreType| -> Result<(), String> {
            *ty = resolve_imported_native_package_type(
                ty,
                &module,
                &imports,
                &opaque,
                &mut HashSet::new(),
            )?;
            Ok(())
        };

        for declaration in &mut core.types {
            if let Some(ty) = &mut declaration.core_body {
                canonicalize(ty)?;
            }
        }
        for constructor in &mut core.constructors {
            for parameter in &mut constructor.params {
                if let Some(ty) = &mut parameter.core_ty {
                    canonicalize(ty)?;
                }
            }
            if let Some(parameter) = &mut constructor.vararg {
                if let Some(ty) = &mut parameter.core_ty {
                    canonicalize(ty)?;
                }
            }
            if let Some(ty) = &mut constructor.core_return_type {
                canonicalize(ty)?;
            }
        }
        for function in &mut core.functions {
            for parameter in &mut function.params {
                if let Some(ty) = &mut parameter.core_ty {
                    canonicalize(ty)?;
                }
            }
            if let Some(ty) = &mut function.core_return_type {
                canonicalize(ty)?;
            }
            for clause in &mut function.clauses {
                if let Some(guard) = clause
                    .guard
                    .as_mut()
                    .and_then(|summary| summary.core_expr.as_mut())
                {
                    canonicalize_native_package_expr(guard, &module, &imports, &opaque)?;
                }
                if let Some(body) = clause.body.core_expr.as_mut() {
                    canonicalize_native_package_expr(body, &module, &imports, &opaque)?;
                }
            }
        }
    }
    Ok(())
}

pub(super) fn resolve_imported_native_package_type(
    ty: &CoreType,
    module: &str,
    imports: &[String],
    aliases: &HashMap<String, (String, CoreType)>,
    visiting: &mut HashSet<String>,
) -> Result<CoreType, String> {
    Ok(match ty {
        CoreType::Named(name) => {
            let Some(key) = imported_native_package_alias(name, module, imports, aliases)? else {
                return Ok(ty.clone());
            };
            if !visiting.insert(key.clone()) {
                return Err(format!(
                    "error[native_ir.native_package_alias_cycle]: opaque package alias `{key}` is recursive"
                ));
            }
            let (_, alias) = &aliases[&key];
            let resolved =
                resolve_imported_native_package_type(alias, module, imports, aliases, visiting)?;
            visiting.remove(&key);
            resolved
        }
        CoreType::Apply { constructor, args } => CoreType::Apply {
            constructor: constructor.clone(),
            args: args
                .iter()
                .map(|arg| {
                    resolve_imported_native_package_type(arg, module, imports, aliases, visiting)
                })
                .collect::<Result<Vec<_>, _>>()?,
        },
        CoreType::List(item) => CoreType::List(Box::new(resolve_imported_native_package_type(
            item, module, imports, aliases, visiting,
        )?)),
        CoreType::Tuple(items) => CoreType::Tuple(
            items
                .iter()
                .map(|item| match item {
                    crate::terlan_typeck::CoreTupleTypeElem::Type(ty) => {
                        resolve_imported_native_package_type(ty, module, imports, aliases, visiting)
                            .map(crate::terlan_typeck::CoreTupleTypeElem::Type)
                    }
                    crate::terlan_typeck::CoreTupleTypeElem::Field { name, ty } => {
                        Ok(crate::terlan_typeck::CoreTupleTypeElem::Field {
                            name: name.clone(),
                            ty: resolve_imported_native_package_type(
                                ty, module, imports, aliases, visiting,
                            )?,
                        })
                    }
                })
                .collect::<Result<Vec<_>, String>>()?,
        ),
        CoreType::Struct { name, fields } => CoreType::Struct {
            name: name.clone(),
            fields: fields
                .iter()
                .cloned()
                .map(|mut field| {
                    field.ty = resolve_imported_native_package_type(
                        &field.ty, module, imports, aliases, visiting,
                    )?;
                    Ok(field)
                })
                .collect::<Result<Vec<_>, String>>()?,
        },
        CoreType::Map(fields) => CoreType::Map(
            fields
                .iter()
                .cloned()
                .map(|mut field| {
                    field.value = resolve_imported_native_package_type(
                        &field.value,
                        module,
                        imports,
                        aliases,
                        visiting,
                    )?;
                    Ok(field)
                })
                .collect::<Result<Vec<_>, String>>()?,
        ),
        CoreType::Arrow {
            params,
            return_type,
        } => CoreType::Arrow {
            params: params
                .iter()
                .map(|parameter| {
                    resolve_imported_native_package_type(
                        parameter, module, imports, aliases, visiting,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
            return_type: Box::new(resolve_imported_native_package_type(
                return_type,
                module,
                imports,
                aliases,
                visiting,
            )?),
        },
        CoreType::Union(types) => CoreType::Union(
            types
                .iter()
                .map(|ty| {
                    resolve_imported_native_package_type(ty, module, imports, aliases, visiting)
                })
                .collect::<Result<Vec<_>, _>>()?,
        ),
        _ => ty.clone(),
    })
}

fn imported_native_package_alias(
    name: &str,
    module: &str,
    imports: &[String],
    aliases: &HashMap<String, (String, CoreType)>,
) -> Result<Option<String>, String> {
    if aliases.contains_key(name) {
        return Ok(Some(name.to_string()));
    }
    let local = format!("{module}.{name}");
    if aliases.contains_key(&local) {
        return Ok(Some(local));
    }
    let mut matches = imports
        .iter()
        .map(|import| format!("{import}.{name}"))
        .filter(|candidate| aliases.contains_key(candidate))
        .collect::<Vec<_>>();
    matches.sort();
    matches.dedup();
    match matches.as_slice() {
        [] => Ok(None),
        [canonical] => Ok(Some(canonical.clone())),
        _ => Err(format!(
            "error[native_ir.ambiguous_native_package_type]: `{name}` in module `{module}` resolves to {}",
            matches.join(", ")
        )),
    }
}

fn canonicalize_native_package_expr(
    expr: &mut CoreExpr,
    module: &str,
    imports: &[String],
    aliases: &HashMap<String, (String, CoreType)>,
) -> Result<(), String> {
    let canonicalize = |ty: &mut CoreType| -> Result<(), String> {
        *ty = resolve_imported_native_package_type(
            ty,
            module,
            imports,
            aliases,
            &mut HashSet::new(),
        )?;
        Ok(())
    };
    match expr {
        CoreExpr::Cast { expr, target_type } => {
            canonicalize_native_package_expr(expr, module, imports, aliases)?;
            canonicalize(target_type)?;
        }
        CoreExpr::Intrinsic(call) => {
            for arg in &mut call.args {
                canonicalize_native_package_expr(arg, module, imports, aliases)?;
            }
            canonicalize(&mut call.return_type)?;
            match &mut call.id {
                CoreIntrinsicId::VmProcessSendMessage(ty)
                | CoreIntrinsicId::VmProcessReceiveMessage(ty)
                | CoreIntrinsicId::VmProcessSpawn(ty)
                | CoreIntrinsicId::VmProcessEntry(ty)
                | CoreIntrinsicId::VmProcessCurrent(ty)
                | CoreIntrinsicId::VmProcessLink(ty)
                | CoreIntrinsicId::VmProcessMonitor(ty)
                | CoreIntrinsicId::VmProcessAcquireResource(ty)
                | CoreIntrinsicId::VmProcessCancel(ty)
                | CoreIntrinsicId::MemoryLayoutOf(ty)
                | CoreIntrinsicId::MemoryShallowSize(ty)
                | CoreIntrinsicId::MemoryRetainedSize(ty) => canonicalize(ty)?,
                CoreIntrinsicId::NativeOperation {
                    parameter_types, ..
                } => {
                    for ty in parameter_types {
                        canonicalize(ty)?;
                    }
                }
                CoreIntrinsicId::Primitive(_) | CoreIntrinsicId::Runtime(_) => {}
            }
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            for item in items {
                canonicalize_native_package_expr(item, module, imports, aliases)?;
            }
        }
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
            canonicalize_native_package_expr(head, module, imports, aliases)?;
            canonicalize_native_package_expr(tail, module, imports, aliases)?;
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            canonicalize_native_package_expr(expr, module, imports, aliases)?;
            for generator in generators {
                canonicalize_native_package_expr(&mut generator.source, module, imports, aliases)?;
            }
            for guard in guards {
                canonicalize_native_package_expr(guard, module, imports, aliases)?;
            }
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                canonicalize_native_package_expr(&mut binding.value, module, imports, aliases)?;
            }
            canonicalize_native_package_expr(body, module, imports, aliases)?;
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                canonicalize_native_package_expr(&mut field.value, module, imports, aliases)?;
            }
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                canonicalize_native_package_expr(&mut field.value, module, imports, aliases)?;
            }
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            canonicalize_native_package_expr(base, module, imports, aliases)?;
            for field in fields {
                canonicalize_native_package_expr(&mut field.value, module, imports, aliases)?;
            }
        }
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Call { args, .. } => {
            for arg in args {
                canonicalize_native_package_expr(arg, module, imports, aliases)?;
            }
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            for arg in args {
                canonicalize_native_package_expr(arg, module, imports, aliases)?;
            }
            canonicalize_native_package_expr(record, module, imports, aliases)?;
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            canonicalize_native_package_expr(receiver, module, imports, aliases)?;
            for arg in args {
                canonicalize_native_package_expr(arg, module, imports, aliases)?;
            }
        }
        CoreExpr::FunctionCall { callee, args } => {
            canonicalize_native_package_expr(callee, module, imports, aliases)?;
            for arg in args {
                canonicalize_native_package_expr(arg, module, imports, aliases)?;
            }
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::UnaryOp { operand: base, .. }
        | CoreExpr::Lam { body: base, .. } => {
            canonicalize_native_package_expr(base, module, imports, aliases)?;
        }
        CoreExpr::Case { scrutinee, clauses } => {
            canonicalize_native_package_expr(scrutinee, module, imports, aliases)?;
            for clause in clauses {
                if let Some(guard) = &mut clause.guard {
                    canonicalize_native_package_expr(guard, module, imports, aliases)?;
                }
                canonicalize_native_package_expr(&mut clause.body, module, imports, aliases)?;
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            canonicalize_native_package_expr(body, module, imports, aliases)?;
            for clause in of_clauses.iter_mut().chain(catch_clauses) {
                if let Some(guard) = &mut clause.guard {
                    canonicalize_native_package_expr(guard, module, imports, aliases)?;
                }
                canonicalize_native_package_expr(&mut clause.body, module, imports, aliases)?;
            }
            if let Some(after) = after_clause {
                canonicalize_native_package_expr(&mut after.trigger, module, imports, aliases)?;
                canonicalize_native_package_expr(&mut after.body, module, imports, aliases)?;
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                canonicalize_native_package_expr(&mut clause.condition, module, imports, aliases)?;
                canonicalize_native_package_expr(&mut clause.body, module, imports, aliases)?;
            }
        }
        CoreExpr::SqlQuery { parameters, .. } => {
            for parameter in parameters {
                canonicalize_native_package_expr(parameter, module, imports, aliases)?;
            }
        }
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
    Ok(())
}

/// Installs a private physical representation for bodyless opaque resources.
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
            let canonical = format!("{}.{}", core.module, declaration.name);
            matches!(
                declaration.visibility,
                crate::terlan_typeck::CoreVisibility::Opaque
            ) && declaration.core_body.is_none()
                && !is_compiler_managed_value_facade(&canonical)
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

/// Admits compatibility layouts for transparent package record aliases.
///
/// Application alias expansion normally carries the concrete struct through
/// native call boundaries. Keeping the checked `Named(package.Type)` layout in
/// the inventory also makes independently encoded package replies readable
/// while older generated helpers migrate to the concrete CoreIR shape.
pub(super) fn native_transparent_record_layouts(
    core: &CoreModule,
) -> Result<Vec<std::sync::Arc<[u8]>>, String> {
    use crate::runtime::native_image::managed::{
        encode_aggregate_layout, ManagedAggregateDescriptor,
    };

    core.types
        .iter()
        .filter(|declaration| {
            !matches!(
                declaration.visibility,
                crate::terlan_typeck::CoreVisibility::Opaque
            )
        })
        .filter_map(|declaration| {
            let CoreType::Struct { fields, .. } = declaration.core_body.as_ref()? else {
                return None;
            };
            Some((declaration, fields))
        })
        .map(|(declaration, fields)| {
            let canonical = format!("{}.{}", core.module, declaration.name);
            let nominal = CoreType::Named(canonical).contract_text();
            let fields = fields
                .iter()
                .map(|field| {
                    super::super::native_type(Some(&field.ty), &field.ty.contract_text())
                        .ok_or_else(|| {
                            format!(
                                "error[native_ir.native_record_layout_type]: unsupported field `{}`",
                                field.ty.contract_text()
                            )
                        })
                        .and_then(super::super::constructors::managed_field_type)
                        .map(|ty| (field.name.clone(), ty))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let descriptor = ManagedAggregateDescriptor::record(&nominal, fields)
                .map_err(|error| format!("error[native_ir.native_record_layout]: {error}"))?;
            encode_aggregate_layout(&descriptor)
                .map(std::sync::Arc::from)
                .map_err(|error| format!("error[native_ir.native_record_layout]: {error}"))
        })
        .collect()
}
