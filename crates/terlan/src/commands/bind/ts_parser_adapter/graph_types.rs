use super::*;

pub(super) fn qualify(graph: &Graph, path: &Path, declaration: &mut TsDeclaration) {
    let file = &graph.files[path];
    match declaration {
        TsDeclaration::Interface(value) => {
            let scope = value.namespace.clone();
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

fn join(left: &str, right: &str) -> String {
    if right.is_empty() {
        left.into()
    } else {
        format!("{left}.{right}")
    }
}

fn resolve(graph: &Graph, path: &Path, scope: &str, bound: &[String], ty: &mut TsTypeRef) {
    walk(ty, &mut |name| {
        if bound.contains(name) || name == "$this" {
            return;
        }
        if let Some(found) = graph.lookup(path, scope, name, &mut BTreeSet::new()) {
            *name = found;
        } else if !matches!(name.as_str(), "Array" | "ReadonlyArray" | "Promise") {
            // An unresolved lexical import must not bind to an unrelated file's
            // identically named type during module-plan qualification.
            *name = format!("{}.unresolved.{name}", graph.files[path].namespace);
        }
    });
}

pub(super) fn walk(ty: &mut TsTypeRef, visit: &mut impl FnMut(&mut String)) {
    match ty {
        TsTypeRef::Named(name) => visit(name),
        TsTypeRef::Generic { name, args } => {
            visit(name);
            for arg in args {
                walk(arg, visit);
            }
        }
        TsTypeRef::Array(value) => walk(value, visit),
        TsTypeRef::Union(values) => {
            for value in values {
                walk(value, visit);
            }
        }
        TsTypeRef::Record(fields) => {
            for field in fields {
                walk(&mut field.ty, visit);
            }
        }
        TsTypeRef::Callback {
            params,
            return_type,
        } => {
            for param in params {
                walk(param, visit);
            }
            walk(return_type, visit);
        }
        _ => {}
    }
}

fn transform_member(
    member: &mut TsInterfaceMember,
    bound: &[String],
    mut visit: impl FnMut(&[String], &mut TsTypeRef),
) {
    match member {
        TsInterfaceMember::Property(value) => visit(bound, &mut value.ty),
        TsInterfaceMember::Method(value) => {
            let mut bound = bound.to_vec();
            bound.extend(value.type_params.clone());
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
    substitute(&mut method.return_type, &renames);
}

fn substitute(ty: &mut TsTypeRef, arguments: &BTreeMap<String, TsTypeRef>) {
    if let TsTypeRef::Named(name) = ty {
        if let Some(argument) = arguments.get(name) {
            *ty = argument.clone();
            return;
        }
    }
    match ty {
        TsTypeRef::Generic { args, .. } | TsTypeRef::Union(args) => {
            for arg in args {
                substitute(arg, arguments);
            }
        }
        TsTypeRef::Array(value) => substitute(value, arguments),
        TsTypeRef::Record(fields) => {
            for field in fields {
                substitute(&mut field.ty, arguments);
            }
        }
        TsTypeRef::Callback {
            params,
            return_type,
        } => {
            for param in params {
                substitute(param, arguments);
            }
            substitute(return_type, arguments);
        }
        _ => {}
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
