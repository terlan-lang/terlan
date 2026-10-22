//! Complete local struct inclusion without recursive compiler calls.

use super::*;

struct StructDefinition {
    parents: Vec<String>,
    fields: Vec<SyntaxStructFieldOutput>,
    span: Span,
}

/// Computes every local parent before its children and reports cycles and
/// field conflicts before changing any declarations. Imported parents already
/// carry the flattened, provider-qualified fields from their interface.
pub(super) fn expand_syntax_struct_includes(
    module: &mut SyntaxModuleOutput,
    resolved: &ResolvedModule,
) -> Vec<Diagnostic> {
    let mut diagnostics = check_syntax_struct_includes(module, resolved);
    if !diagnostics.is_empty() {
        return diagnostics;
    }
    let definitions = module
        .declarations
        .iter()
        .filter_map(|declaration| {
            let SyntaxDeclarationPayload::Struct {
                name,
                includes,
                fields,
                ..
            } = &declaration.payload
            else {
                return None;
            };
            Some((
                name.clone(),
                StructDefinition {
                    parents: includes.clone(),
                    fields: fields.clone(),
                    span: declaration.span.into(),
                },
            ))
        })
        .collect::<HashMap<_, _>>();
    let imported = imported_struct_fields(resolved);
    let mut completed: HashMap<String, Option<Vec<SyntaxStructFieldOutput>>> = HashMap::new();
    let mut active = HashSet::new();
    let mut path = Vec::new();
    let mut cyclic = HashSet::new();
    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Struct { name, .. } = &declaration.payload else {
            continue;
        };
        let mut pending = vec![(name.clone(), false)];
        while let Some((name, finish)) = pending.pop() {
            if completed.contains_key(&name) {
                continue;
            }
            let definition = &definitions[&name];
            if !finish {
                if active.contains(&name) {
                    let start = path
                        .iter()
                        .position(|item| item == &name)
                        .expect("active struct is on the path");
                    cyclic.extend(path[start..].iter().cloned());
                    let mut cycle = path[start..].to_vec();
                    cycle.push(name);
                    diagnostics.push(Diagnostic {
                        span: definitions[path.last().expect("cycle has a parent")].span,
                        message: format!("struct include cycle: {}", cycle.join(" -> ")),
                        severity: DiagSeverity::Error,
                    });
                    continue;
                }
                active.insert(name.clone());
                path.push(name.clone());
                pending.push((name, true));
                pending.extend(
                    definition
                        .parents
                        .iter()
                        .rev()
                        .filter(|parent| definitions.contains_key(*parent))
                        .map(|parent| (parent.clone(), false)),
                );
                continue;
            }
            active.remove(&name);
            debug_assert_eq!(path.last(), Some(&name));
            path.pop();
            if cyclic.contains(&name) {
                completed.insert(name, None);
                continue;
            }
            let mut fields = Vec::new();
            let mut field_names = definition
                .fields
                .iter()
                .map(|field| field.name.clone())
                .collect::<HashSet<_>>();
            let mut valid = true;
            for parent in &definition.parents {
                let parent_fields = if definitions.contains_key(parent) {
                    completed.get(parent).and_then(Option::as_ref)
                } else {
                    imported.get(parent)
                };
                let Some(parent_fields) = parent_fields else {
                    valid = false;
                    continue;
                };
                for field in parent_fields {
                    if !field_names.insert(field.name.clone()) {
                        diagnostics.push(Diagnostic {
                            span: definition.span,
                            message: format!("included struct `{parent}` field `{}` conflicts with declaration of struct `{name}`", field.name),
                            severity: DiagSeverity::Error,
                        });
                        valid = false;
                    } else {
                        fields.push(field.clone());
                    }
                }
            }
            fields.extend(definition.fields.clone());
            completed.insert(name, valid.then_some(fields));
        }
    }
    if diagnostics.is_empty() {
        for declaration in &mut module.declarations {
            if let SyntaxDeclarationPayload::Struct { name, fields, .. } = &mut declaration.payload
            {
                if let Some(Some(expanded)) = completed.remove(name) {
                    *fields = expanded;
                }
            }
        }
    }
    diagnostics
}

/// Collects imported struct fields visible under local import names.
///
/// Inputs:
/// - `resolved`: resolved module context with imported type items and provider
///   interfaces.
///
/// Output:
/// - Map from local imported type name to ordered syntax-output-like field
///   metadata.
///
/// Transformation:
/// - Reads public struct field signatures from the provider interface and
///   converts them back into syntax-output field metadata with empty spans and
///   no defaults. Qualifies provider type names while preserving generic
///   parameters so child declarations cannot rebind inherited field types.
pub(super) fn imported_struct_fields(
    resolved: &ResolvedModule,
) -> HashMap<String, Vec<SyntaxStructFieldOutput>> {
    let mut structs = HashMap::new();

    for (local_name, imported) in &resolved.imported_types {
        let Some(interface) = resolved.interface_map.get(&imported.source_module) else {
            continue;
        };
        let Some(fields) = interface.struct_fields.get(&imported.source_name) else {
            continue;
        };
        let mut provider_types = interface
            .public_types
            .iter()
            .chain(interface.traits.keys())
            .map(|name| (name.clone(), format!("{}.{}", interface.module, name)))
            .collect::<HashMap<_, _>>();
        for parameter in interface
            .type_params
            .get(&imported.source_name)
            .into_iter()
            .flatten()
        {
            provider_types.remove(&normalize_type_param_name(parameter));
        }

        structs.insert(
            local_name.clone(),
            fields
                .iter()
                .map(|field| SyntaxStructFieldOutput {
                    name: field.name.clone(),
                    annotation: SyntaxTypeOutput {
                        text: crate::terlan_hir::qualify_syntax_type_text(
                            &field.annotation,
                            &provider_types,
                        ),
                        span: Default::default(),
                    },
                    is_private: field.is_private,
                    docs: Vec::new(),
                    has_default: false,
                    default: None,
                    span: Default::default(),
                })
                .collect(),
        );
    }

    structs
}
