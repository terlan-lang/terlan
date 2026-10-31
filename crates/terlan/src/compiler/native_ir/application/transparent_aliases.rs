//! Resolves transparent source aliases to concrete CoreIR storage types.

#[path = "transparent_aliases/types.rs"]
mod types;
use types::*;

use std::collections::{HashMap, HashSet};

use crate::terlan_typeck::{
    core_type_from_text, CoreExpr, CoreIntrinsicId, CoreModule, CorePattern, CoreRecordExprField,
    CoreTupleTypeElem, CoreType,
};

#[cfg(test)]
#[path = "transparent_aliases_test.rs"]
mod tests;

#[derive(Clone)]
struct Alias {
    module: String,
    imports: Vec<String>,
    params: Vec<String>,
    body: CoreType,
    recursive: bool,
}

pub(super) fn expand_transparent_aliases(cores: &mut [CoreModule]) {
    expand_aliases(cores, true);
}

/// Expands aliases introduced after the initial application-wide pass without
/// unrolling recursive storage aliases by another structural layer.
pub(super) fn expand_generated_transparent_aliases(cores: &mut [CoreModule]) {
    expand_aliases(cores, false);
}

fn expand_aliases(cores: &mut [CoreModule], expand_recursive: bool) {
    let mut aliases = cores
        .iter()
        .flat_map(|core| {
            let imports = core
                .imports
                .iter()
                .map(|import| import.module.clone())
                .collect::<Vec<_>>();
            core.types.iter().filter_map(move |declaration| {
                let canonical = format!("{}.{}", core.module, declaration.name);
                let task =
                    super::super::task_values::declaration_storage(&canonical, &declaration.params);
                let body = task.as_ref().or(declaration.core_body.as_ref())?;
                if task.is_none()
                    && matches!(
                        declaration.visibility,
                        crate::terlan_typeck::CoreVisibility::Opaque
                    )
                {
                    return None;
                }
                let body = super::super::effect_values::storage_type(&canonical, body);
                Some((
                    canonical,
                    Alias {
                        module: core.module.clone(),
                        imports: imports.clone(),
                        params: declaration.params.clone(),
                        body,
                        recursive: false,
                    },
                ))
            })
        })
        .collect::<HashMap<_, _>>();
    mark_recursive_aliases(&mut aliases);
    for core in cores.iter_mut() {
        let module = core.module.clone();
        let imports = core
            .imports
            .iter()
            .map(|import| import.module.clone())
            .collect::<Vec<_>>();
        for declaration in &mut core.types {
            // The Effect intrinsic survives pruning of its source `run`
            // declaration. Retain the same expanded generic schema used by its
            // operands so result inference can use the canonical type instead.
            if module == "std.core.Effect" && declaration.name == "Effect" {
                if let Some(body) = &mut declaration.core_body {
                    *body = resolve(
                        body,
                        &module,
                        &imports,
                        &aliases,
                        &mut HashSet::new(),
                        expand_recursive,
                    );
                }
            }
            if let Some(CoreType::Struct { fields, .. }) = &mut declaration.core_body {
                for field in fields {
                    field.ty = resolve(
                        &field.ty,
                        &module,
                        &imports,
                        &aliases,
                        &mut HashSet::new(),
                        expand_recursive,
                    );
                }
            }
        }
        for constructor in &mut core.constructors {
            for parameter in &mut constructor.params {
                if let Some(ty) = &mut parameter.core_ty {
                    *ty = resolve(
                        ty,
                        &module,
                        &imports,
                        &aliases,
                        &mut HashSet::new(),
                        expand_recursive,
                    );
                }
            }
            if let Some(parameter) = &mut constructor.vararg {
                if let Some(ty) = &mut parameter.core_ty {
                    *ty = resolve(
                        ty,
                        &module,
                        &imports,
                        &aliases,
                        &mut HashSet::new(),
                        expand_recursive,
                    );
                }
            }
            if let Some(ty) = &mut constructor.core_return_type {
                *ty = resolve(
                    ty,
                    &module,
                    &imports,
                    &aliases,
                    &mut HashSet::new(),
                    expand_recursive,
                );
            }
        }
        for function in &mut core.functions {
            for parameter in &mut function.params {
                if parameter.core_ty.is_none() {
                    parameter.core_ty = core_type_from_source_text(&parameter.ty);
                }
                if let Some(ty) = &mut parameter.core_ty {
                    *ty = resolve(
                        ty,
                        &module,
                        &imports,
                        &aliases,
                        &mut HashSet::new(),
                        expand_recursive,
                    );
                }
            }
            if function.core_return_type.is_none() {
                function.core_return_type = core_type_from_source_text(&function.return_type);
            }
            if let Some(ty) = &mut function.core_return_type {
                *ty = resolve(
                    ty,
                    &module,
                    &imports,
                    &aliases,
                    &mut HashSet::new(),
                    expand_recursive,
                );
            }
            for clause in &mut function.clauses {
                for pattern in clause.core_patterns.iter_mut().flatten() {
                    resolve_pattern(pattern, &module, &imports, &aliases);
                }
                if let Some(guard) = clause
                    .guard
                    .as_mut()
                    .and_then(|summary| summary.core_expr.as_mut())
                {
                    resolve_expr(guard, &module, &imports, &aliases, expand_recursive);
                }
                if let Some(body) = clause.body.core_expr.as_mut() {
                    resolve_expr(body, &module, &imports, &aliases, expand_recursive);
                }
            }
        }
    }
}

fn resolve(
    ty: &CoreType,
    module: &str,
    imports: &[String],
    aliases: &HashMap<String, Alias>,
    visiting: &mut HashSet<String>,
    expand_recursive: bool,
) -> CoreType {
    let rebuilt = match ty {
        // Unit's public alias and source sentinel share one native storage type,
        // including inside callback signatures and generic aggregate arguments.
        CoreType::AtomLiteral(value) if value == "unit" => CoreType::Named("Unit".into()),
        CoreType::Apply { constructor, args } => {
            let args = args
                .iter()
                .map(|arg| resolve(arg, module, imports, aliases, visiting, expand_recursive))
                .collect::<Vec<_>>();
            // The standard opaque List owns the builtin list representation.
            // Qualification must not introduce a second managed identity, and
            // unrelated nominal types named List must remain distinct.
            if constructor == "std.collections.List.List" && args.len() == 1 {
                return CoreType::List(Box::new(args[0].clone()));
            }
            if let Some((key, alias)) = find_alias(constructor, module, imports, aliases) {
                if alias.recursive && !expand_recursive {
                    return CoreType::Apply {
                        constructor: key,
                        args,
                    };
                }
                // Nominal struct applications keep their type arguments in
                // their managed identity; only transparent aliases erase them.
                if matches!(alias.body, CoreType::Struct { .. }) && !alias.params.is_empty() {
                    return CoreType::Apply {
                        constructor: key,
                        args,
                    };
                }
                if alias.params.len() == args.len() && visiting.insert(key.clone()) {
                    let values = alias
                        .params
                        .iter()
                        .cloned()
                        .zip(args)
                        .collect::<HashMap<_, _>>();
                    let substituted = substitute(&alias.body, &values);
                    let resolved = resolve(
                        &substituted,
                        &alias.module,
                        &alias.imports,
                        aliases,
                        visiting,
                        expand_recursive,
                    );
                    visiting.remove(&key);
                    return resolved;
                }
            }
            CoreType::Apply {
                constructor: constructor.clone(),
                args,
            }
        }
        CoreType::Named(name) => {
            if let Some((key, alias)) = find_alias(name, module, imports, aliases) {
                if alias.recursive && !expand_recursive {
                    return CoreType::Named(key);
                }
                if alias.params.is_empty() && visiting.insert(key.clone()) {
                    let resolved = resolve(
                        &alias.body,
                        &alias.module,
                        &alias.imports,
                        aliases,
                        visiting,
                        expand_recursive,
                    );
                    visiting.remove(&key);
                    return resolved;
                }
            }
            ty.clone()
        }
        CoreType::List(element) => CoreType::List(Box::new(resolve(
            element,
            module,
            imports,
            aliases,
            visiting,
            expand_recursive,
        ))),
        CoreType::Tuple(elements) => CoreType::Tuple(
            elements
                .iter()
                .map(|element| match element {
                    CoreTupleTypeElem::Type(ty) => CoreTupleTypeElem::Type(resolve(
                        ty,
                        module,
                        imports,
                        aliases,
                        visiting,
                        expand_recursive,
                    )),
                    CoreTupleTypeElem::Field { name, ty } => CoreTupleTypeElem::Field {
                        name: name.clone(),
                        ty: resolve(ty, module, imports, aliases, visiting, expand_recursive),
                    },
                })
                .collect(),
        ),
        CoreType::Struct { name, fields } => CoreType::Struct {
            name: name.clone(),
            fields: fields
                .iter()
                .cloned()
                .map(|mut field| {
                    field.ty = resolve(
                        &field.ty,
                        module,
                        imports,
                        aliases,
                        visiting,
                        expand_recursive,
                    );
                    field
                })
                .collect(),
        },
        CoreType::Map(fields) => CoreType::Map(
            fields
                .iter()
                .cloned()
                .map(|mut field| {
                    field.value = resolve(
                        &field.value,
                        module,
                        imports,
                        aliases,
                        visiting,
                        expand_recursive,
                    );
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
                .map(|ty| resolve(ty, module, imports, aliases, visiting, expand_recursive))
                .collect(),
            return_type: Box::new(resolve(
                return_type,
                module,
                imports,
                aliases,
                visiting,
                expand_recursive,
            )),
        },
        CoreType::Union(types) => {
            CoreType::Union(super::super::structured_case::canonical_atom_union(
                types
                    .iter()
                    .map(|ty| resolve(ty, module, imports, aliases, visiting, expand_recursive))
                    .collect(),
            ))
        }
        _ => ty.clone(),
    };
    rebuilt
}

fn resolve_expr(
    expr: &mut CoreExpr,
    module: &str,
    imports: &[String],
    aliases: &HashMap<String, Alias>,
    expand_recursive: bool,
) {
    let mut resolve_type = |ty: &mut CoreType| {
        *ty = resolve(
            ty,
            module,
            imports,
            aliases,
            &mut HashSet::new(),
            expand_recursive,
        );
    };
    match expr {
        CoreExpr::Cast { expr, target_type } => {
            resolve_type(target_type);
            if let CoreExpr::ConstructorCall {
                type_args,
                constructor,
                constructor_identity,
                args,
            } = expr.as_mut()
            {
                type_args.iter_mut().for_each(&mut resolve_type);
                for arg in args.iter_mut() {
                    resolve_expr(arg, module, imports, aliases, expand_recursive);
                }
                let identity = constructor_identity.as_deref().unwrap_or(constructor);
                if transparent_alias_constructor_tag(identity, args.len(), module, imports, aliases)
                    .is_some()
                {
                    return;
                }
            }
            resolve_expr(expr, module, imports, aliases, expand_recursive);
        }
        CoreExpr::Intrinsic(call) => {
            for arg in &mut call.args {
                resolve_expr(arg, module, imports, aliases, expand_recursive);
            }
            resolve_type(&mut call.return_type);
            match &mut call.id {
                CoreIntrinsicId::VmProcessSendMessage(ty)
                | CoreIntrinsicId::VmProcessReceiveMessage(ty)
                | CoreIntrinsicId::VmProcessSpawn(ty)
                | CoreIntrinsicId::VmProcessEntry(ty)
                | CoreIntrinsicId::VmProcessCurrent(ty)
                | CoreIntrinsicId::VmProcessLink(ty)
                | CoreIntrinsicId::VmProcessUnlink(ty)
                | CoreIntrinsicId::VmProcessMonitor(ty)
                | CoreIntrinsicId::VmProcessDemonitor(ty)
                | CoreIntrinsicId::VmProcessAcquireResource(ty)
                | CoreIntrinsicId::VmProcessReleaseResource(ty)
                | CoreIntrinsicId::VmProcessCancel(ty)
                | CoreIntrinsicId::MemoryLayoutOf(ty)
                | CoreIntrinsicId::MemoryShallowSize(ty)
                | CoreIntrinsicId::MemoryRetainedSize(ty)
                | CoreIntrinsicId::ErasedValueIs(ty) => resolve_type(ty),
                CoreIntrinsicId::NativeOperation {
                    parameter_types, ..
                } => parameter_types.iter_mut().for_each(&mut resolve_type),
                CoreIntrinsicId::Primitive(_)
                | CoreIntrinsicId::Runtime(_)
                | CoreIntrinsicId::VmEffectFail => {}
            }
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::Array(items) => {
            items
                .iter_mut()
                .for_each(|item| resolve_expr(item, module, imports, aliases, expand_recursive));
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
            resolve_expr(head, module, imports, aliases, expand_recursive);
            resolve_expr(tail, module, imports, aliases, expand_recursive);
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            resolve_expr(expr, module, imports, aliases, expand_recursive);
            for generator in generators {
                resolve_pattern(&mut generator.pattern, module, imports, aliases);
                resolve_expr(
                    &mut generator.source,
                    module,
                    imports,
                    aliases,
                    expand_recursive,
                );
            }
            for guard in guards {
                resolve_expr(guard, module, imports, aliases, expand_recursive);
            }
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                resolve_pattern(&mut binding.pattern, module, imports, aliases);
                resolve_expr(
                    &mut binding.value,
                    module,
                    imports,
                    aliases,
                    expand_recursive,
                );
            }
            resolve_expr(body, module, imports, aliases, expand_recursive);
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                resolve_expr(&mut field.value, module, imports, aliases, expand_recursive);
            }
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                resolve_expr(&mut field.value, module, imports, aliases, expand_recursive);
            }
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            resolve_expr(base, module, imports, aliases, expand_recursive);
            for field in fields {
                resolve_expr(&mut field.value, module, imports, aliases, expand_recursive);
            }
        }
        CoreExpr::ConstructorCall {
            type_args,
            constructor,
            constructor_identity,
            args,
        } => {
            type_args.iter_mut().for_each(&mut resolve_type);
            for arg in args.iter_mut() {
                resolve_expr(arg, module, imports, aliases, expand_recursive);
            }
            let identity = constructor_identity.as_deref().unwrap_or(constructor);
            if let Some(record) = explicit_struct_constructor(
                identity,
                type_args,
                args,
                module,
                imports,
                aliases,
                expand_recursive,
            ) {
                *expr = record;
                return;
            }
            if let Some((canonical, alias)) = find_alias(identity, module, imports, aliases) {
                if matches!(
                    canonical.as_str(),
                    "std.core.Effect.Mapped" | "std.core.Effect.FlatMap"
                ) {
                    // Retain the canonical checked storage witness even for
                    // these non-generic aliases. Effect callback admission must
                    // not depend on which helper constructed the descriptor.
                    let target_type = resolve(
                        &alias.body,
                        &alias.module,
                        &alias.imports,
                        aliases,
                        &mut HashSet::new(),
                        expand_recursive,
                    );
                    let tag = if canonical == "std.core.Effect.Mapped" {
                        "mapped"
                    } else {
                        "flat_map"
                    };
                    let mut items = vec![CoreExpr::Atom(tag.to_string())];
                    items.append(args);
                    *expr = CoreExpr::Cast {
                        expr: Box::new(CoreExpr::Tuple(items)),
                        target_type,
                    };
                    return;
                }
            }
            if let Some(tag) =
                transparent_alias_constructor_tag(identity, args.len(), module, imports, aliases)
            {
                let target_type = (!type_args.is_empty()).then(|| {
                    resolve(
                        &CoreType::Apply {
                            constructor: identity.to_string(),
                            args: type_args.clone(),
                        },
                        module,
                        imports,
                        aliases,
                        &mut HashSet::new(),
                        expand_recursive,
                    )
                });
                let mut items = Vec::with_capacity(args.len().saturating_add(1));
                items.push(CoreExpr::Atom(tag));
                items.append(args);
                let tuple = CoreExpr::Tuple(items);
                *expr = match target_type {
                    Some(target_type) => CoreExpr::Cast {
                        expr: Box::new(tuple),
                        target_type,
                    },
                    None => tuple,
                };
            }
        }
        CoreExpr::RemoteCall {
            type_args, args, ..
        }
        | CoreExpr::Call {
            type_args, args, ..
        } => {
            for ty in type_args {
                *ty = resolve(
                    ty,
                    module,
                    imports,
                    aliases,
                    &mut HashSet::new(),
                    expand_recursive,
                );
            }
            for arg in args {
                resolve_expr(arg, module, imports, aliases, expand_recursive);
            }
        }
        CoreExpr::ConstructorChain {
            type_args,
            args,
            record,
            ..
        } => {
            type_args.iter_mut().for_each(&mut resolve_type);
            for arg in args {
                resolve_expr(arg, module, imports, aliases, expand_recursive);
            }
            resolve_expr(record, module, imports, aliases, expand_recursive);
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            resolve_expr(receiver, module, imports, aliases, expand_recursive);
            for arg in args {
                resolve_expr(arg, module, imports, aliases, expand_recursive);
            }
        }
        CoreExpr::FunctionCall { callee, args } => {
            resolve_expr(callee, module, imports, aliases, expand_recursive);
            for arg in args {
                resolve_expr(arg, module, imports, aliases, expand_recursive);
            }
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::UnaryOp { operand: base, .. } => {
            resolve_expr(base, module, imports, aliases, expand_recursive)
        }
        CoreExpr::Lam {
            params,
            parameter_types,
            body,
        } => {
            for pattern in params {
                resolve_pattern(pattern, module, imports, aliases);
            }
            parameter_types
                .iter_mut()
                .flatten()
                .for_each(&mut resolve_type);
            resolve_expr(body, module, imports, aliases, expand_recursive);
        }
        CoreExpr::Case { scrutinee, clauses } => {
            resolve_expr(scrutinee, module, imports, aliases, expand_recursive);
            for clause in clauses {
                resolve_pattern(&mut clause.pattern, module, imports, aliases);
                if let Some(guard) = &mut clause.guard {
                    resolve_expr(guard, module, imports, aliases, expand_recursive);
                }
                resolve_expr(&mut clause.body, module, imports, aliases, expand_recursive);
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            resolve_expr(body, module, imports, aliases, expand_recursive);
            for clause in of_clauses.iter_mut().chain(catch_clauses) {
                resolve_pattern(&mut clause.pattern, module, imports, aliases);
                if let Some(guard) = &mut clause.guard {
                    resolve_expr(guard, module, imports, aliases, expand_recursive);
                }
                resolve_expr(&mut clause.body, module, imports, aliases, expand_recursive);
            }
            if let Some(after) = after_clause {
                resolve_expr(
                    &mut after.trigger,
                    module,
                    imports,
                    aliases,
                    expand_recursive,
                );
                resolve_expr(&mut after.body, module, imports, aliases, expand_recursive);
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                resolve_expr(
                    &mut clause.condition,
                    module,
                    imports,
                    aliases,
                    expand_recursive,
                );
                resolve_expr(&mut clause.body, module, imports, aliases, expand_recursive);
            }
        }
        CoreExpr::SqlQuery {
            parameters,
            result_core_type,
            ..
        } => {
            resolve_type(result_core_type);
            for parameter in parameters {
                resolve_expr(parameter, module, imports, aliases, expand_recursive);
            }
        }
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
}

/// Materializes a generic struct with its nominal application and checked
/// field witnesses, rather than the declaration's unresolved layout template.
fn explicit_struct_constructor(
    identity: &str,
    type_args: &[CoreType],
    args: &[CoreExpr],
    module: &str,
    imports: &[String],
    aliases: &HashMap<String, Alias>,
    expand_recursive: bool,
) -> Option<CoreExpr> {
    if type_args.is_empty() {
        return None;
    }
    let (canonical, alias) = find_alias(identity, module, imports, aliases)?;
    let CoreType::Struct { fields, .. } = &alias.body else {
        return None;
    };
    if fields.len() != args.len() || alias.params.len() != type_args.len() {
        return None;
    }
    let values = alias
        .params
        .iter()
        .cloned()
        .zip(type_args.iter().cloned())
        .collect();
    let fields = fields
        .iter()
        .zip(args)
        .map(|(field, value)| CoreRecordExprField {
            key: field.name.clone(),
            required: true,
            value: CoreExpr::Cast {
                expr: Box::new(value.clone()),
                target_type: resolve(
                    &substitute(&field.ty, &values),
                    &alias.module,
                    &alias.imports,
                    aliases,
                    &mut HashSet::new(),
                    expand_recursive,
                ),
            },
        })
        .collect();
    Some(CoreExpr::Cast {
        expr: Box::new(CoreExpr::RecordConstruct {
            name: canonical.clone(),
            fields,
        }),
        target_type: CoreType::Apply {
            constructor: canonical,
            args: type_args.to_vec(),
        },
    })
}

fn transparent_alias_constructor_tag(
    identity: &str,
    arity: usize,
    module: &str,
    imports: &[String],
    aliases: &HashMap<String, Alias>,
) -> Option<String> {
    let (_, alias) = find_alias(identity, module, imports, aliases)?;
    let CoreType::Tuple(elements) = &alias.body else {
        return None;
    };
    if elements.len() != arity.saturating_add(1) {
        return None;
    }
    match elements.first()? {
        CoreTupleTypeElem::Type(CoreType::AtomLiteral(tag))
        | CoreTupleTypeElem::Field {
            ty: CoreType::AtomLiteral(tag),
            ..
        } => Some(tag.clone()),
        _ => None,
    }
}

fn resolve_pattern(
    pattern: &mut CorePattern,
    module: &str,
    imports: &[String],
    aliases: &HashMap<String, Alias>,
) {
    match pattern {
        CorePattern::Constructor {
            name,
            constructor_identity,
            args,
        } => {
            for arg in args.iter_mut() {
                resolve_pattern(arg, module, imports, aliases);
            }
            let identity = constructor_identity.as_deref().unwrap_or(name);
            if let Some(tag) =
                transparent_alias_constructor_tag(identity, args.len(), module, imports, aliases)
            {
                let mut items = Vec::with_capacity(args.len().saturating_add(1));
                items.push(CorePattern::Atom(tag));
                items.append(args);
                *pattern = CorePattern::Tuple(items);
            }
        }
        CorePattern::Tuple(items) | CorePattern::List(items) => {
            for item in items {
                resolve_pattern(item, module, imports, aliases);
            }
        }
        CorePattern::Alias { pattern, .. } => {
            resolve_pattern(pattern, module, imports, aliases);
        }
        CorePattern::ListCons { head, tail } => {
            resolve_pattern(head, module, imports, aliases);
            resolve_pattern(tail, module, imports, aliases);
        }
        CorePattern::Map(fields) => {
            for field in fields {
                resolve_pattern(&mut field.value, module, imports, aliases);
            }
        }
        CorePattern::Record { fields, .. } => {
            for field in fields {
                resolve_pattern(&mut field.value, module, imports, aliases);
            }
        }
        CorePattern::Wildcard
        | CorePattern::Var(_)
        | CorePattern::Int(_)
        | CorePattern::Float(_)
        | CorePattern::String(_)
        | CorePattern::StringPattern(_)
        | CorePattern::Atom(_)
        | CorePattern::BinaryLayout { .. } => {}
    }
}
