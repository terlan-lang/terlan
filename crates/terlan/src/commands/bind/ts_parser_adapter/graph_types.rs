use super::*;
use crate::commands::bind::ts_type_mapping::TsConstructorUtility;

pub(super) fn qualify(graph: &Graph, path: &Path, declaration: &mut TsDeclaration) {
    let file = &graph.files[path];
    match declaration {
        TsDeclaration::Interface(value) => {
            let scope = value.namespace.clone();
            qualify_defaults(
                graph,
                path,
                &scope,
                &value.type_params,
                &mut value.type_defaults,
            );
            let reserved = value.type_params.iter().cloned().collect();
            for member in &mut value.members {
                avoid_capture(member, &reserved);
            }
            for parent in &mut value.parents {
                resolve(graph, path, &scope, &value.type_params, parent);
            }
            for member in &mut value.members {
                transform_member(member, &value.type_params, |bound, ty| {
                    resolve(graph, path, &scope, bound, ty)
                });
            }
            value.namespace = join(&file.namespace, &scope);
        }
        TsDeclaration::TypeAlias(value) => {
            let scope = value.namespace.clone();
            for constraint in value.type_constraints.iter_mut().flatten() {
                resolve(graph, path, &scope, &value.type_params, constraint);
            }
            qualify_defaults(
                graph,
                path,
                &scope,
                &value.type_params,
                &mut value.type_defaults,
            );
            if value.imported {
                let name = match &value.ty {
                    TsTypeRef::Named(name) | TsTypeRef::Generic { name, .. } => name,
                    _ => "",
                };
                value.imported = graph
                    .lookup(path, &scope, name, &mut BTreeSet::new())
                    .is_none();
            }
            resolve(graph, path, &scope, &value.type_params, &mut value.ty);
            value.namespace = join(&file.namespace, &scope);
        }
        TsDeclaration::Unsupported(value) => value.source = join(&file.namespace, &value.source),
    }
}

fn qualify_defaults(
    graph: &Graph,
    path: &Path,
    scope: &str,
    parameters: &[String],
    defaults: &mut [Option<TsTypeRef>],
) {
    for (index, default) in defaults.iter_mut().enumerate() {
        if let Some(default) = default {
            resolve(graph, path, scope, &parameters[..index], default);
        }
    }
}

pub(super) fn join(left: &str, right: &str) -> String {
    if right.is_empty() {
        left.into()
    } else {
        format!("{left}.{right}")
    }
}

fn resolve(graph: &Graph, path: &Path, scope: &str, bound: &[String], ty: &mut TsTypeRef) {
    resolve_standard_utilities(graph, path, bound, ty);
    mark_distributive_conditionals(ty, bound);
    walk(ty, &mut |name| {
        if bound.contains(name) || name == "$this" {
            return;
        }
        if let Some(found) = graph
            .lookup(path, scope, name, &mut BTreeSet::new())
            .or_else(|| graph.browser_type(path, scope, name))
        {
            *name = found;
        } else if !matches!(name.as_str(), "Array" | "ReadonlyArray" | "Promise") {
            // An unresolved lexical import must not bind to an unrelated file's
            // identically named type during module-plan qualification.
            *name = format!("{}.unresolved.{name}", graph.files[path].namespace);
        }
    });
}

fn resolve_standard_utilities(graph: &Graph, path: &Path, bound: &[String], ty: &mut TsTypeRef) {
    if matches!(ty, TsTypeRef::JsonParseResult)
        && graph.has_unshadowed_standard_name(path, "ReturnType")
        && graph.has_unshadowed_standard_name(path, "JSON")
    {
        // The standard declaration's JSON.parse result is `any`. Preserve
        // that fact for type relations without mapping it to a binding value.
        *ty = TsTypeRef::Any;
    }
    if let TsTypeRef::Generic { name, args } = ty {
        let utility = match name.as_str() {
            "ConstructorParameters" => Some(TsConstructorUtility::Parameters),
            "InstanceType" => Some(TsConstructorUtility::Instance),
            _ => None,
        };
        if let Some(utility) = utility {
            if args.len() == 1
                && !bound.contains(name)
                && graph.has_unshadowed_standard_name(path, name)
            {
                *ty = TsTypeRef::ConstructorUtility(utility, Box::new(args.pop().unwrap()));
            }
        }
    }
    ty.for_each_child_mut(|child| resolve_standard_utilities(graph, path, bound, child));
}

fn mark_distributive_conditionals(ty: &mut TsTypeRef, bound: &[String]) {
    if let TsTypeRef::Conditional(conditional) = ty {
        if let TsTypeRef::Named(name) = &conditional.check {
            if bound.contains(name) {
                conditional.distribute_over = Some(name.clone());
            }
        }
    }
    ty.for_each_child_mut(|child| mark_distributive_conditionals(child, bound));
}

pub(super) fn walk(ty: &mut TsTypeRef, visit: &mut impl FnMut(&mut String)) {
    match ty {
        TsTypeRef::Named(name) | TsTypeRef::Generic { name, .. } => visit(name),
        _ => {}
    }
    ty.for_each_child_mut(|child| walk(child, visit));
}

pub(super) fn transform_member(
    member: &mut TsInterfaceMember,
    bound: &[String],
    mut visit: impl FnMut(&[String], &mut TsTypeRef),
) {
    match member {
        TsInterfaceMember::Property(value) => visit(bound, &mut value.ty),
        TsInterfaceMember::Method(value) => {
            let mut bound = bound.to_vec();
            bound.extend(value.type_params.clone());
            for constraint in value.type_constraints.iter_mut().flatten() {
                visit(&bound, constraint);
            }
            for param in &mut value.params {
                visit(&bound, &mut param.ty);
            }
            visit(&bound, &mut value.return_type);
        }
        TsInterfaceMember::Unsupported(_) => {}
    }
}

fn avoid_capture(member: &mut TsInterfaceMember, reserved: &BTreeSet<String>) {
    let TsInterfaceMember::Method(method) = member else {
        return;
    };
    let mut renames = BTreeMap::new();
    let mut used = reserved
        .iter()
        .chain(&method.type_params)
        .cloned()
        .collect::<BTreeSet<_>>();
    for parameter in &mut method.type_params {
        if reserved.contains(parameter) {
            let mut replacement = format!("{parameter}Method");
            while !used.insert(replacement.clone()) {
                replacement.push('_');
            }
            renames.insert(parameter.clone(), TsTypeRef::Named(replacement.clone()));
            *parameter = replacement;
        }
    }
    for param in &mut method.params {
        substitute(&mut param.ty, &renames);
    }
    for constraint in method.type_constraints.iter_mut().flatten() {
        substitute(constraint, &renames);
    }
    substitute(&mut method.return_type, &renames);
}

pub(super) fn substitute(ty: &mut TsTypeRef, arguments: &BTreeMap<String, TsTypeRef>) {
    if let TsTypeRef::Named(name) = ty {
        if let Some(argument) = arguments.get(name) {
            *ty = argument.clone();
            return;
        }
    }
    if let TsTypeRef::Conditional(conditional) = ty {
        if let Some((name, argument)) = conditional
            .distribute_over
            .as_ref()
            .and_then(|name| arguments.get(name).map(|argument| (name.clone(), argument)))
        {
            match argument {
                TsTypeRef::Never => {
                    *ty = TsTypeRef::Never;
                    return;
                }
                TsTypeRef::Union(members) => {
                    let mut replacements = arguments.clone();
                    conditional.distribute_over = None;
                    let members = members
                        .iter()
                        .map(|member| {
                            replacements.insert(name.clone(), member.clone());
                            let mut branch = TsTypeRef::Conditional(conditional.clone());
                            substitute(&mut branch, &replacements);
                            branch
                        })
                        .collect();
                    *ty = TsTypeRef::Union(members);
                    return;
                }
                TsTypeRef::Named(new_name) if !new_name.contains('.') => {
                    conditional.distribute_over = Some(new_name.clone());
                }
                // A named alias can still resolve to a union. Keep the
                // distributivity marker until the evaluator sees its shape.
                TsTypeRef::Named(_) | TsTypeRef::Generic { .. } => {}
                _ => conditional.distribute_over = None,
            }
        }
    }
    ty.for_each_child_mut(|child| substitute(child, arguments));
}

type GenericParameters = Vec<(String, Option<TsTypeRef>)>;

pub(super) fn apply_generic_defaults(declarations: &mut [TsDeclaration]) {
    let mut inventory = BTreeMap::<String, GenericParameters>::new();
    for declaration in &*declarations {
        let (namespace, name, parameters, defaults) = match declaration {
            TsDeclaration::Interface(value) => (
                &value.namespace,
                &value.name,
                &value.type_params,
                &value.type_defaults,
            ),
            TsDeclaration::TypeAlias(value) => (
                &value.namespace,
                &value.name,
                &value.type_params,
                &value.type_defaults,
            ),
            TsDeclaration::Unsupported(_) => continue,
        };
        let name = join(
            namespace,
            &super::super::super::ts_dom_module_mapping::terlan_type_name(name),
        );
        let merged = inventory.entry(name).or_insert_with(|| {
            parameters
                .iter()
                .cloned()
                .map(|parameter| (parameter, None))
                .collect()
        });
        for ((_, existing), default) in merged.iter_mut().zip(defaults) {
            if existing.is_none() {
                *existing = default.clone();
            }
        }
    }

    let apply = |ty: &mut TsTypeRef| apply_defaults(ty, &inventory, &mut BTreeSet::new());
    for declaration in declarations {
        match declaration {
            TsDeclaration::Interface(value) => {
                for parent in &mut value.parents {
                    apply(parent);
                }
                for member in &mut value.members {
                    transform_member(member, &value.type_params, |_, ty| apply(ty));
                }
            }
            TsDeclaration::TypeAlias(value) => apply(&mut value.ty),
            TsDeclaration::Unsupported(_) => {}
        }
    }
}

fn apply_defaults(
    ty: &mut TsTypeRef,
    inventory: &BTreeMap<String, GenericParameters>,
    expanding: &mut BTreeSet<String>,
) {
    let (name, arguments) = match ty {
        TsTypeRef::Named(name) => (name, None),
        TsTypeRef::Generic { name, args } => {
            for argument in args.iter_mut() {
                apply_defaults(argument, inventory, expanding);
            }
            (name, Some(args))
        }
        _ => return ty.for_each_child_mut(|child| apply_defaults(child, inventory, expanding)),
    };
    let Some(parameters) = inventory.get(name) else {
        return;
    };
    let supplied = arguments.as_ref().map_or(0, |arguments| arguments.len());
    if supplied == parameters.len() {
        return;
    }
    *ty = if supplied > parameters.len() || !expanding.insert(name.clone()) {
        TsTypeRef::UnresolvedGenericArguments(format!(
            "cannot apply {supplied} type arguments to {name}"
        ))
    } else {
        let arguments = arguments.map(std::mem::take).unwrap_or_default();
        let result = append_defaults(name, arguments, parameters, inventory, expanding);
        expanding.remove(name);
        result
    };
}

fn append_defaults(
    name: &str,
    mut arguments: Vec<TsTypeRef>,
    parameters: &[(String, Option<TsTypeRef>)],
    inventory: &BTreeMap<String, GenericParameters>,
    expanding: &mut BTreeSet<String>,
) -> TsTypeRef {
    let mut supplied = parameters
        .iter()
        .map(|(name, _)| name.clone())
        .zip(arguments.iter().cloned())
        .collect::<BTreeMap<_, _>>();
    for (parameter, default) in &parameters[arguments.len()..] {
        let Some(mut default) = default.clone() else {
            return TsTypeRef::UnresolvedGenericArguments(format!(
                "{name} requires a type argument for {parameter}"
            ));
        };
        substitute(&mut default, &supplied);
        apply_defaults(&mut default, inventory, expanding);
        supplied.insert(parameter.clone(), default.clone());
        arguments.push(default);
    }
    TsTypeRef::Generic {
        name: name.into(),
        args: arguments,
    }
}

pub(super) fn inherit(declarations: &mut Vec<TsDeclaration>) {
    let mut merged = Vec::new();
    let mut indexes = BTreeMap::new();
    for declaration in std::mem::take(declarations) {
        if let TsDeclaration::Interface(value) = declaration {
            let name = join(&value.namespace, &value.name);
            if let Some(index) = indexes.get(&name).copied() {
                let TsDeclaration::Interface(target) = &mut merged[index] else {
                    unreachable!()
                };
                target.plain_record &= value.plain_record;
                if target.type_params != value.type_params {
                    target.members.push(failure(format!(
                        "incompatible merged type parameters for {name}"
                    )));
                } else {
                    target.members.extend(value.members);
                    target.parents.extend(value.parents);
                }
            } else {
                indexes.insert(name, merged.len());
                merged.push(TsDeclaration::Interface(value));
            }
        } else {
            merged.push(declaration);
        }
    }
    *declarations = merged;
    let inventory = declarations
        .iter()
        .filter_map(|declaration| match declaration {
            TsDeclaration::Interface(value) => Some((
                join(
                    &value.namespace,
                    &super::super::super::ts_dom_module_mapping::terlan_type_name(&value.name),
                ),
                declaration.clone(),
            )),
            TsDeclaration::TypeAlias(value) => Some((
                join(
                    &value.namespace,
                    &super::super::super::ts_dom_module_mapping::terlan_type_name(&value.name),
                ),
                declaration.clone(),
            )),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    for declaration in declarations {
        let TsDeclaration::Interface(value) = declaration else {
            continue;
        };
        let name = join(
            &value.namespace,
            &super::super::super::ts_dom_module_mapping::terlan_type_name(&value.name),
        );
        value.members = flatten(
            &TsTypeRef::Named(name.clone()),
            &inventory,
            &mut BTreeSet::new(),
        )
        .unwrap_or_else(|reason| vec![failure(reason)]);
        let this = if value.type_params.is_empty() {
            TsTypeRef::Named(name)
        } else {
            TsTypeRef::Generic {
                name,
                args: value
                    .type_params
                    .iter()
                    .cloned()
                    .map(TsTypeRef::Named)
                    .collect(),
            }
        };
        let substitutions = BTreeMap::from([("$this".into(), this)]);
        for member in &mut value.members {
            transform_member(member, &[], |_, ty| substitute(ty, &substitutions));
        }
    }
}

fn failure(reason: String) -> TsInterfaceMember {
    TsInterfaceMember::Unsupported(TsUnsupportedMember {
        source: "extends".into(),
        reason: "ts_bindgen.unresolved_heritage",
        detail: reason,
    })
}

fn flatten(
    ty: &TsTypeRef,
    inventory: &BTreeMap<String, TsDeclaration>,
    seen: &mut BTreeSet<String>,
) -> Result<Vec<TsInterfaceMember>, String> {
    let (name, args) = match ty {
        TsTypeRef::Named(name) => (name, &[][..]),
        TsTypeRef::Generic { name, args } => (name, args.as_slice()),
        TsTypeRef::UnresolvedGenericArguments(reason) => return Err(reason.clone()),
        _ => return Err("base declaration is not a named type".into()),
    };
    if !seen.insert(name.clone()) {
        return Err(format!("cyclic base declaration {name}"));
    }
    let declaration = inventory
        .get(name)
        .ok_or_else(|| format!("unavailable base declaration {name}"))?;
    let (params, parents, mut members) = match declaration {
        TsDeclaration::Interface(value) => (
            &value.type_params,
            value.parents.clone(),
            value.members.clone(),
        ),
        TsDeclaration::TypeAlias(value) => (&value.type_params, vec![value.ty.clone()], Vec::new()),
        _ => unreachable!(),
    };
    let arguments = params
        .iter()
        .cloned()
        .zip(args.iter().cloned())
        .collect::<BTreeMap<_, _>>();
    let mut reserved = params.iter().cloned().collect::<BTreeSet<_>>();
    for argument in args {
        walk(&mut argument.clone(), &mut |name| {
            reserved.insert(name.clone());
        });
    }
    for member in &mut members {
        avoid_capture(member, &reserved);
    }
    let own = members
        .iter()
        .filter_map(|member| match member {
            TsInterfaceMember::Property(value) => Some(value.name.clone()),
            TsInterfaceMember::Method(value) => Some(value.name.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    for mut parent in parents {
        substitute(&mut parent, &arguments);
        match flatten(&parent, inventory, &mut seen.clone()) {
            Ok(inherited) => members.extend(inherited.into_iter().filter(|member| match member {
                TsInterfaceMember::Property(value) => !own.contains(&value.name),
                TsInterfaceMember::Method(value) => !own.contains(&value.name),
                _ => true,
            })),
            Err(reason) => members.push(failure(reason)),
        }
    }
    for member in &mut members {
        transform_member(member, &[], |bound, ty| {
            let substitutions = arguments
                .iter()
                .filter(|(name, _)| !bound.contains(name))
                .map(|(name, ty)| (name.clone(), ty.clone()))
                .collect();
            substitute(ty, &substitutions);
        });
    }
    Ok(members)
}
