use super::*;

pub(super) fn mark_recursive_aliases(aliases: &mut HashMap<String, Alias>) {
    let graph = aliases
        .iter()
        .map(|(key, alias)| {
            let mut references = HashSet::new();
            collect_alias_references(
                &alias.body,
                &alias.module,
                &alias.imports,
                aliases,
                &mut references,
            );
            (key.clone(), references)
        })
        .collect::<HashMap<_, _>>();
    let recursive = graph
        .keys()
        .filter(|start| {
            let mut pending = graph[*start].iter().collect::<Vec<_>>();
            let mut visited = HashSet::new();
            while let Some(candidate) = pending.pop() {
                if candidate == *start {
                    return true;
                }
                if visited.insert(candidate) {
                    pending.extend(graph.get(candidate).into_iter().flatten());
                }
            }
            false
        })
        .cloned()
        .collect::<HashSet<_>>();
    for key in recursive {
        if let Some(alias) = aliases.get_mut(&key) {
            alias.recursive = true;
        }
    }
}

pub(super) fn collect_alias_references(
    ty: &CoreType,
    module: &str,
    imports: &[String],
    aliases: &HashMap<String, Alias>,
    references: &mut HashSet<String>,
) {
    match ty {
        CoreType::Named(name) => {
            if let Some((key, _)) = find_alias(name, module, imports, aliases) {
                references.insert(key);
            }
        }
        CoreType::Apply { constructor, args } => {
            if let Some((key, _)) = find_alias(constructor, module, imports, aliases) {
                references.insert(key);
            }
            for arg in args {
                collect_alias_references(arg, module, imports, aliases, references);
            }
        }
        CoreType::List(element) => {
            collect_alias_references(element, module, imports, aliases, references);
        }
        CoreType::Tuple(elements) => {
            for element in elements {
                collect_alias_references(
                    match element {
                        CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => ty,
                    },
                    module,
                    imports,
                    aliases,
                    references,
                );
            }
        }
        CoreType::Struct { fields, .. } => {
            for field in fields {
                collect_alias_references(&field.ty, module, imports, aliases, references);
            }
        }
        CoreType::Map(fields) => {
            for field in fields {
                collect_alias_references(&field.value, module, imports, aliases, references);
            }
        }
        CoreType::Arrow {
            params,
            return_type,
        } => {
            for param in params {
                collect_alias_references(param, module, imports, aliases, references);
            }
            collect_alias_references(return_type, module, imports, aliases, references);
        }
        CoreType::Union(types) => {
            for ty in types {
                collect_alias_references(ty, module, imports, aliases, references);
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
        | CoreType::AtomLiteral(_) => {}
    }
}

/// Parses backend-relevant source types whose generic arguments include
/// compile-time integer constants. CoreType intentionally has no executable
/// numeric type, so the constant is retained as a nominal substitution token;
/// transparent aliases such as `Bits[3] = BinaryDescriptor` erase it while an
/// ABI that truly depends on the constant remains unsupported.
pub(super) fn core_type_from_source_text(text: &str) -> Option<CoreType> {
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

pub(super) fn split_type_arguments(text: &str) -> Option<Vec<&str>> {
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

pub(super) fn find_alias<'a>(
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

pub(super) fn substitute(ty: &CoreType, values: &HashMap<String, CoreType>) -> CoreType {
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
