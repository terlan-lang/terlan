use super::*;

/// Lowers resolver interface-map keys into deterministic CoreIR imports.
///
/// Inputs:
/// - `resolved`: resolver artifact containing the visible interface map.
///
/// Output:
/// - Sorted Core import summaries.
///
/// Transformation:
/// - Converts module names into backend-neutral Core import records without
///   preserving backend or filesystem details.
pub(crate) fn lower_core_imports(resolved: &ResolvedModule) -> Vec<CoreImport> {
    let mut imports = resolved
        .interface_map
        .keys()
        .filter(|module| *module != &resolved.name)
        .map(|module| CoreImport {
            module: module.clone(),
            kind: CoreImportKind::Module,
        })
        .collect::<Vec<_>>();
    imports.sort_by(|left, right| left.module.cmp(&right.module));
    imports
}

/// Lowers public interface members into deterministic CoreIR exports.
///
/// Inputs:
/// - `interface`: module interface produced by resolution/typechecking.
///
/// Output:
/// - Sorted Core export summaries.
///
/// Transformation:
/// - Records public functions, public types, and public constructors as
///   backend-independent export summaries.
pub(crate) fn lower_core_exports(interface: &ModuleInterface) -> Vec<CoreExport> {
    let mut exports = Vec::new();
    exports.extend(
        interface
            .functions
            .iter()
            .filter_map(|((name, arity), signature)| {
                signature.public.then(|| CoreExport {
                    name: name.clone(),
                    kind: CoreExportKind::Function { arity: *arity },
                })
            }),
    );
    exports.extend(interface.public_types.iter().map(|name| CoreExport {
        name: name.clone(),
        kind: CoreExportKind::Type,
    }));
    exports.extend(
        interface
            .constructors
            .iter()
            .flat_map(|(name, signatures)| {
                signatures.iter().filter_map(move |signature| {
                    signature.public.then(|| CoreExport {
                        name: name.clone(),
                        kind: CoreExportKind::Constructor {
                            min_arity: signature.min_arity,
                        },
                    })
                })
            }),
    );
    exports.sort_by(|left, right| {
        let left_key = core_export_sort_key(left);
        let right_key = core_export_sort_key(right);
        left_key.cmp(&right_key)
    });
    exports
}

/// Builds a deterministic sort key for a Core export.
///
/// Inputs:
/// - `export`: Core export summary.
///
/// Output:
/// - Stable string key.
///
/// Transformation:
/// - Combines export kind, name, and arity-like data into a sortable identity.
fn core_export_sort_key(export: &CoreExport) -> String {
    match export.kind {
        CoreExportKind::Function { arity } => format!("function:{}:{}", export.name, arity),
        CoreExportKind::Type => format!("type:{}", export.name),
        CoreExportKind::Constructor { min_arity } => {
            format!("constructor:{}:{}", export.name, min_arity)
        }
    }
}

/// Lowers interface type declarations into deterministic CoreIR type summaries.
///
/// Inputs:
/// - `interface`: module interface produced by resolution/typechecking.
///
/// Output:
/// - Sorted Core type declarations.
///
/// Transformation:
/// - Preserves type visibility, type parameters, textual type body summary,
///   and an optional typed CoreType body when the declaration body is already
///   representable by the current CoreType model.
pub(crate) fn lower_core_types(interface: &ModuleInterface) -> Vec<CoreTypeDecl> {
    let mut names = interface
        .public_types
        .iter()
        .chain(interface.private_types.iter())
        .chain(interface.opaque_types.iter())
        .cloned()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let visibility = if interface.opaque_types.contains(&name) {
                CoreVisibility::Opaque
            } else if interface.public_types.contains(&name) {
                CoreVisibility::Public
            } else {
                CoreVisibility::Private
            };
            let body = interface
                .type_bodies
                .get(&name)
                .cloned()
                .unwrap_or_default();
            let core_body = core_type_from_body_variants(&body);
            CoreTypeDecl {
                params: interface
                    .type_params
                    .get(&name)
                    .cloned()
                    .unwrap_or_default(),
                body,
                core_body,
                name,
                visibility,
            }
        })
        .collect()
}

/// Builds typed CoreType bodies for local syntax-output struct declarations.
///
/// Inputs:
/// - `module`: compiler-facing syntax module whose declarations may include
///   local structs.
///
/// Output:
/// - Map from struct name to typed `CoreType::Struct` payload for structs whose
///   field annotations all lower into supported CoreType forms.
///
/// Transformation:
/// - Scans local struct declarations, lowers each field annotation through the
///   existing type-text CoreType converter, and keeps only fully typed
///   structural bodies.
pub(crate) fn core_syntax_struct_type_bodies(
    module: &SyntaxModuleOutput,
) -> HashMap<String, CoreType> {
    module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Struct { name, fields, .. } => {
                core_type_from_syntax_struct_fields(name, fields)
            }
            _ => None,
        })
        .collect()
}

/// Converts one syntax-output struct declaration into a typed Core struct body.
///
/// Inputs:
/// - `name`: local struct type name.
/// - `fields`: syntax-output struct fields with source type annotations.
///
/// Output:
/// - `Some((name, CoreType::Struct))` when every field annotation is
///   representable by the current CoreType model.
/// - `None` when any field still requires unsupported type lowering.
///
/// Transformation:
/// - Preserves field order, lowers annotation text into backend-neutral
///   CoreType payloads, and avoids encoding runtime struct construction.
fn core_type_from_syntax_struct_fields(
    name: &str,
    fields: &[SyntaxStructFieldOutput],
) -> Option<(String, CoreType)> {
    fields
        .iter()
        .map(|field| {
            core_type_from_text(&field.annotation.text).map(|ty| CoreStructTypeField {
                name: field.name.clone(),
                ty,
                is_private: field.is_private,
            })
        })
        .collect::<Option<Vec<_>>>()
        .map(|fields| {
            (
                name.to_string(),
                CoreType::Struct {
                    name: name.to_string(),
                    fields,
                },
            )
        })
}

/// Lowers interface function signatures into deterministic CoreIR functions.
///
/// Inputs:
/// - `interface`: module interface produced by resolution/typechecking.
///
/// Output:
/// - Sorted Core function summaries.
///
/// Transformation:
/// - Converts function signature metadata into typed Core parameter and return
///   summaries without lowering to any backend call form.
pub(crate) fn lower_core_functions(interface: &ModuleInterface) -> Vec<CoreFunction> {
    let mut functions = interface
        .functions
        .iter()
        .map(|((name, arity), signature)| CoreFunction {
            name: name.clone(),
            arity: *arity,
            public: signature.public,
            params: signature
                .params
                .iter()
                .map(|param| CoreParam {
                    name: param.name.clone(),
                    ty: param.annotation.clone(),
                    core_ty: core_type_from_text(&param.annotation),
                })
                .collect(),
            return_type: signature.return_type.clone(),
            core_return_type: core_type_from_text(&signature.return_type),
            clauses: Vec::new(),
        })
        .collect::<Vec<_>>();
    functions.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.arity.cmp(&right.arity))
    });
    functions
}

/// Lowers interface constructor signatures into deterministic CoreIR
/// constructors.
///
/// Inputs:
/// - `interface`: module interface produced by resolution/typechecking.
///
/// Output:
/// - Sorted Core constructor summaries.
///
/// Transformation:
/// - Converts constructor signatures into semantic constructor declarations
///   without committing to tuple, atom, record, or backend layout encoding.
pub(crate) fn lower_core_constructors(interface: &ModuleInterface) -> Vec<CoreConstructorDecl> {
    let mut constructors = interface
        .constructors
        .iter()
        .flat_map(|(name, signatures)| {
            signatures.iter().map(move |signature| CoreConstructorDecl {
                name: name.clone(),
                public: signature.public,
                min_arity: signature.min_arity,
                params: signature
                    .params
                    .iter()
                    .map(|param| CoreParam {
                        name: param.name.clone(),
                        ty: param.annotation.clone(),
                        core_ty: core_type_from_text(&param.annotation),
                    })
                    .collect(),
                vararg: signature.vararg.as_ref().map(|param| CoreParam {
                    name: param.name.clone(),
                    ty: param.annotation.clone(),
                    core_ty: core_type_from_text(&param.annotation),
                }),
                return_type: signature.return_type.clone(),
                core_return_type: core_type_from_text(&signature.return_type),
            })
        })
        .collect::<Vec<_>>();
    constructors.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.min_arity.cmp(&right.min_arity))
    });
    constructors
}
