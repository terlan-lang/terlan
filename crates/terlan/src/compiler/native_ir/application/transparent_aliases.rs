//! Resolves transparent source aliases to concrete CoreIR storage types.

use std::collections::{HashMap, HashSet};

use crate::terlan_typeck::{
    core_type_from_text, CoreExpr, CoreIntrinsicId, CoreModule, CorePattern, CoreTupleTypeElem,
    CoreType,
};

#[derive(Clone)]
struct Alias {
    module: String,
    params: Vec<String>,
    body: CoreType,
}

pub(super) fn expand_transparent_aliases(cores: &mut [CoreModule]) {
    let aliases = cores
        .iter()
        .flat_map(|core| {
            core.types.iter().filter_map(move |declaration| {
                let body = declaration.core_body.as_ref()?;
                if matches!(
                    declaration.visibility,
                    crate::terlan_typeck::CoreVisibility::Opaque
                ) {
                    return None;
                }
                let canonical = format!("{}.{}", core.module, declaration.name);
                Some((
                    canonical,
                    Alias {
                        module: core.module.clone(),
                        params: declaration.params.clone(),
                        body: body.clone(),
                    },
                ))
            })
        })
        .collect::<HashMap<_, _>>();
    if aliases.is_empty() {
        return;
    }

    for core in cores {
        let module = core.module.clone();
        let imports = core
            .imports
            .iter()
            .map(|import| import.module.clone())
            .collect::<Vec<_>>();
        for declaration in &mut core.types {
            if let Some(CoreType::Struct { fields, .. }) = &mut declaration.core_body {
                for field in fields {
                    field.ty = resolve(&field.ty, &module, &imports, &aliases, &mut HashSet::new());
                }
            }
        }
        for constructor in &mut core.constructors {
            for parameter in &mut constructor.params {
                if let Some(ty) = &mut parameter.core_ty {
                    *ty = resolve(ty, &module, &imports, &aliases, &mut HashSet::new());
                }
            }
            if let Some(parameter) = &mut constructor.vararg {
                if let Some(ty) = &mut parameter.core_ty {
                    *ty = resolve(ty, &module, &imports, &aliases, &mut HashSet::new());
                }
            }
            if let Some(ty) = &mut constructor.core_return_type {
                *ty = resolve(ty, &module, &imports, &aliases, &mut HashSet::new());
            }
        }
        for function in &mut core.functions {
            for parameter in &mut function.params {
                if parameter.core_ty.is_none() {
                    parameter.core_ty = core_type_from_source_text(&parameter.ty);
                }
                if let Some(ty) = &mut parameter.core_ty {
                    *ty = resolve(ty, &module, &imports, &aliases, &mut HashSet::new());
                }
            }
            if function.core_return_type.is_none() {
                function.core_return_type = core_type_from_source_text(&function.return_type);
            }
            if let Some(ty) = &mut function.core_return_type {
                *ty = resolve(ty, &module, &imports, &aliases, &mut HashSet::new());
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
                    resolve_expr(guard, &module, &imports, &aliases);
                }
                if let Some(body) = clause.body.core_expr.as_mut() {
                    resolve_expr(body, &module, &imports, &aliases);
                }
            }
        }
    }
}

/// Parses backend-relevant source types whose generic arguments include
/// compile-time integer constants. CoreType intentionally has no executable
/// numeric type, so the constant is retained as a nominal substitution token;
/// transparent aliases such as `Bits[3] = BinaryDescriptor` erase it while an
/// ABI that truly depends on the constant remains unsupported.
fn core_type_from_source_text(text: &str) -> Option<CoreType> {
    core_type_from_text(text).or_else(|| {
        let text = text.trim();
        let open = text.find('[')?;
        let constructor = text[..open].trim();
        let inner = text.get(open + 1..text.len().checked_sub(1)?)?;
        if !text.ends_with(']')
            || constructor.is_empty()
            || !constructor
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.'))
        {
            return None;
        }
        let args = split_type_arguments(inner)?
            .into_iter()
            .map(|arg| {
                core_type_from_source_text(arg).or_else(|| {
                    arg.chars()
                        .all(|ch| ch.is_ascii_digit())
                        .then(|| CoreType::Named(format!("$const:{arg}")))
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(CoreType::Apply {
            constructor: constructor.to_string(),
            args,
        })
    })
}

fn split_type_arguments(text: &str) -> Option<Vec<&str>> {
    let mut depth = 0_usize;
    let mut start = 0_usize;
    let mut args = Vec::new();
    for (index, ch) in text.char_indices() {
        match ch {
            '[' | '{' | '(' => depth = depth.checked_add(1)?,
            ']' | '}' | ')' => depth = depth.checked_sub(1)?,
            ',' if depth == 0 => {
                let arg = text[start..index].trim();
                if arg.is_empty() {
                    return None;
                }
                args.push(arg);
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    let arg = text[start..].trim();
    if arg.is_empty() {
        return None;
    }
    args.push(arg);
    Some(args)
}

fn resolve(
    ty: &CoreType,
    module: &str,
    imports: &[String],
    aliases: &HashMap<String, Alias>,
    visiting: &mut HashSet<String>,
) -> CoreType {
    let rebuilt = match ty {
        CoreType::Apply { constructor, args } => {
            let args = args
                .iter()
                .map(|arg| resolve(arg, module, imports, aliases, visiting))
                .collect::<Vec<_>>();
            if let Some((key, alias)) = find_alias(constructor, module, imports, aliases) {
                if alias.params.len() == args.len() && visiting.insert(key.clone()) {
                    let values = alias
                        .params
                        .iter()
                        .cloned()
                        .zip(args)
                        .collect::<HashMap<_, _>>();
                    let substituted = substitute(&alias.body, &values);
                    let resolved = resolve(&substituted, &alias.module, imports, aliases, visiting);
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
                if alias.params.is_empty() && visiting.insert(key.clone()) {
                    let resolved = resolve(&alias.body, &alias.module, imports, aliases, visiting);
                    visiting.remove(&key);
                    return resolved;
                }
            }
            ty.clone()
        }
        CoreType::List(element) => CoreType::List(Box::new(resolve(
            element, module, imports, aliases, visiting,
        ))),
        CoreType::Tuple(elements) => CoreType::Tuple(
            elements
                .iter()
                .map(|element| match element {
                    CoreTupleTypeElem::Type(ty) => {
                        CoreTupleTypeElem::Type(resolve(ty, module, imports, aliases, visiting))
                    }
                    CoreTupleTypeElem::Field { name, ty } => CoreTupleTypeElem::Field {
                        name: name.clone(),
                        ty: resolve(ty, module, imports, aliases, visiting),
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
                    field.ty = resolve(&field.ty, module, imports, aliases, visiting);
                    field
                })
                .collect(),
        },
        CoreType::Map(fields) => CoreType::Map(
            fields
                .iter()
                .cloned()
                .map(|mut field| {
                    field.value = resolve(&field.value, module, imports, aliases, visiting);
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
                .map(|ty| resolve(ty, module, imports, aliases, visiting))
                .collect(),
            return_type: Box::new(resolve(return_type, module, imports, aliases, visiting)),
        },
        CoreType::Union(types) => CoreType::Union(
            types
                .iter()
                .map(|ty| resolve(ty, module, imports, aliases, visiting))
                .collect(),
        ),
        _ => ty.clone(),
    };
    rebuilt
}

fn find_alias<'a>(
    name: &str,
    module: &str,
    imports: &[String],
    aliases: &'a HashMap<String, Alias>,
) -> Option<(String, &'a Alias)> {
    if let Some(alias) = aliases.get(name) {
        return Some((name.to_string(), alias));
    }
    let local = format!("{module}.{name}");
    if let Some(alias) = aliases.get(&local) {
        return Some((local, alias));
    }
    let mut matches = imports
        .iter()
        .map(|import| format!("{import}.{name}"))
        .filter_map(|key| aliases.get(&key).map(|alias| (key, alias)))
        .collect::<Vec<_>>();
    if matches.len() == 1 {
        return matches.pop();
    }
    let suffix = format!(".{name}");
    let mut matches = aliases
        .iter()
        .filter(|(key, _)| key.ends_with(&suffix))
        .map(|(key, alias)| (key.clone(), alias));
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn substitute(ty: &CoreType, values: &HashMap<String, CoreType>) -> CoreType {
    match ty {
        CoreType::Named(name) => values.get(name).cloned().unwrap_or_else(|| ty.clone()),
        CoreType::Apply { constructor, args } => CoreType::Apply {
            constructor: constructor.clone(),
            args: args.iter().map(|ty| substitute(ty, values)).collect(),
        },
        CoreType::List(element) => CoreType::List(Box::new(substitute(element, values))),
        CoreType::Tuple(elements) => CoreType::Tuple(
            elements
                .iter()
                .map(|element| match element {
                    CoreTupleTypeElem::Type(ty) => CoreTupleTypeElem::Type(substitute(ty, values)),
                    CoreTupleTypeElem::Field { name, ty } => CoreTupleTypeElem::Field {
                        name: name.clone(),
                        ty: substitute(ty, values),
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
                    field.ty = substitute(&field.ty, values);
                    field
                })
                .collect(),
        },
        CoreType::Map(fields) => CoreType::Map(
            fields
                .iter()
                .cloned()
                .map(|mut field| {
                    field.value = substitute(&field.value, values);
                    field
                })
                .collect(),
        ),
        CoreType::Arrow {
            params,
            return_type,
        } => CoreType::Arrow {
            params: params.iter().map(|ty| substitute(ty, values)).collect(),
            return_type: Box::new(substitute(return_type, values)),
        },
        CoreType::Union(types) => {
            CoreType::Union(types.iter().map(|ty| substitute(ty, values)).collect())
        }
        _ => ty.clone(),
    }
}

fn resolve_expr(
    expr: &mut CoreExpr,
    module: &str,
    imports: &[String],
    aliases: &HashMap<String, Alias>,
) {
    let mut resolve_type = |ty: &mut CoreType| {
        *ty = resolve(ty, module, imports, aliases, &mut HashSet::new());
    };
    match expr {
        CoreExpr::Cast { expr, target_type } => {
            resolve_type(target_type);
            if let CoreExpr::ConstructorCall {
                constructor,
                constructor_identity,
                args,
            } = expr.as_mut()
            {
                for arg in args.iter_mut() {
                    resolve_expr(arg, module, imports, aliases);
                }
                let identity = constructor_identity.as_deref().unwrap_or(constructor);
                if transparent_alias_constructor_tag(identity, args.len(), module, imports, aliases)
                    .is_some()
                {
                    return;
                }
            }
            resolve_expr(expr, module, imports, aliases);
        }
        CoreExpr::Intrinsic(call) => {
            for arg in &mut call.args {
                resolve_expr(arg, module, imports, aliases);
            }
            resolve_type(&mut call.return_type);
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
                | CoreIntrinsicId::MemoryRetainedSize(ty) => resolve_type(ty),
                CoreIntrinsicId::NativeOperation {
                    parameter_types, ..
                } => parameter_types.iter_mut().for_each(&mut resolve_type),
                CoreIntrinsicId::Primitive(_) | CoreIntrinsicId::Runtime(_) => {}
            }
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            items
                .iter_mut()
                .for_each(|item| resolve_expr(item, module, imports, aliases));
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
            resolve_expr(head, module, imports, aliases);
            resolve_expr(tail, module, imports, aliases);
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            resolve_expr(expr, module, imports, aliases);
            for generator in generators {
                resolve_expr(&mut generator.source, module, imports, aliases);
            }
            for guard in guards {
                resolve_expr(guard, module, imports, aliases);
            }
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                resolve_expr(&mut binding.value, module, imports, aliases);
            }
            resolve_expr(body, module, imports, aliases);
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                resolve_expr(&mut field.value, module, imports, aliases);
            }
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                resolve_expr(&mut field.value, module, imports, aliases);
            }
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            resolve_expr(base, module, imports, aliases);
            for field in fields {
                resolve_expr(&mut field.value, module, imports, aliases);
            }
        }
        CoreExpr::ConstructorCall {
            constructor,
            constructor_identity,
            args,
        } => {
            for arg in args.iter_mut() {
                resolve_expr(arg, module, imports, aliases);
            }
            let identity = constructor_identity.as_deref().unwrap_or(constructor);
            if let Some(tag) =
                transparent_alias_constructor_tag(identity, args.len(), module, imports, aliases)
            {
                let mut items = Vec::with_capacity(args.len().saturating_add(1));
                items.push(CoreExpr::Atom(tag));
                items.append(args);
                *expr = CoreExpr::Tuple(items);
            }
        }
        CoreExpr::RemoteCall { args, .. } | CoreExpr::Call { args, .. } => {
            for arg in args {
                resolve_expr(arg, module, imports, aliases);
            }
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            for arg in args {
                resolve_expr(arg, module, imports, aliases);
            }
            resolve_expr(record, module, imports, aliases);
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            resolve_expr(receiver, module, imports, aliases);
            for arg in args {
                resolve_expr(arg, module, imports, aliases);
            }
        }
        CoreExpr::FunctionCall { callee, args } => {
            resolve_expr(callee, module, imports, aliases);
            for arg in args {
                resolve_expr(arg, module, imports, aliases);
            }
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::UnaryOp { operand: base, .. }
        | CoreExpr::Lam { body: base, .. } => resolve_expr(base, module, imports, aliases),
        CoreExpr::Case { scrutinee, clauses } => {
            resolve_expr(scrutinee, module, imports, aliases);
            for clause in clauses {
                resolve_pattern(&mut clause.pattern, module, imports, aliases);
                if let Some(guard) = &mut clause.guard {
                    resolve_expr(guard, module, imports, aliases);
                }
                resolve_expr(&mut clause.body, module, imports, aliases);
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            resolve_expr(body, module, imports, aliases);
            for clause in of_clauses.iter_mut().chain(catch_clauses) {
                if let Some(guard) = &mut clause.guard {
                    resolve_expr(guard, module, imports, aliases);
                }
                resolve_expr(&mut clause.body, module, imports, aliases);
            }
            if let Some(after) = after_clause {
                resolve_expr(&mut after.trigger, module, imports, aliases);
                resolve_expr(&mut after.body, module, imports, aliases);
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                resolve_expr(&mut clause.condition, module, imports, aliases);
                resolve_expr(&mut clause.body, module, imports, aliases);
            }
        }
        CoreExpr::SqlQuery {
            parameters,
            result_core_type,
            ..
        } => {
            resolve_type(result_core_type);
            for parameter in parameters {
                resolve_expr(parameter, module, imports, aliases);
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
