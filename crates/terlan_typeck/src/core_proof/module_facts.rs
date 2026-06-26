use super::*;

/// Collects explicit source imports into deterministic CoreIR import summaries.
///
/// Inputs:
/// - `module`: compiler-facing syntax output.
///
/// Output:
/// - Sorted Core import summaries for imports written in source.
///
/// Transformation:
/// - Converts syntax-output import declarations into backend-neutral module
///   imports and excludes implicit/builtin interface-map entries.
pub(crate) fn core_syntax_imports(module: &SyntaxModuleOutput) -> Vec<CoreImport> {
    let mut imports = module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Import {
                import_kind,
                module_name,
                items,
                source_path,
                is_type,
                ..
            } => Some(CoreImport {
                module: core_import_identity(import_kind, module_name, items, source_path),
                kind: core_import_kind(*import_kind, *is_type),
            }),
            _ => None,
        })
        .collect::<Vec<_>>();
    imports.sort_by(|left, right| {
        left.module
            .cmp(&right.module)
            .then_with(|| format!("{:?}", left.kind).cmp(&format!("{:?}", right.kind)))
    });
    imports.dedup_by(|left, right| left.module == right.module && left.kind == right.kind);
    imports
}

/// Collects provider modules that were resolved through type or trait imports.
///
/// Inputs:
/// - `resolved`: resolver artifact containing imported type and trait aliases.
///
/// Output:
/// - Core type-module imports for the actual provider modules backing those
///   aliases.
///
/// Transformation:
/// - Converts alias-level resolver facts such as `Task -> std.core.Task.Task`
///   into type-module CoreIR imports such as `std.core.Task`. Executable source
///   imports are preserved separately by `core_syntax_imports`, so resolved
///   type facts do not force runtime target-profile support by themselves.
pub(crate) fn core_resolved_imported_modules(resolved: &ResolvedModule) -> Vec<CoreImport> {
    let mut imports = resolved
        .imported_types
        .values()
        .chain(resolved.imported_traits.values())
        .map(|imported| CoreImport {
            module: imported.source_module.clone(),
            kind: CoreImportKind::TypeModule,
        })
        .collect::<Vec<_>>();
    imports.sort_by(|left, right| left.module.cmp(&right.module));
    imports.dedup_by(|left, right| left.module == right.module && left.kind == right.kind);
    imports
}

/// Merges CoreIR imports while preserving deterministic order and uniqueness.
///
/// Inputs:
/// - `imports`: mutable base import list.
/// - `extra`: additional imports discovered after initial syntax lowering.
///
/// Output:
/// - No direct return value; `imports` is sorted and deduplicated in place.
///
/// Transformation:
/// - Appends resolved-provider imports to syntax imports, then normalizes by
///   module identity and import kind so contract text remains stable.
pub(crate) fn merge_core_imports(imports: &mut Vec<CoreImport>, extra: Vec<CoreImport>) {
    imports.extend(extra);
    imports.sort_by(|left, right| {
        left.module
            .cmp(&right.module)
            .then_with(|| format!("{:?}", left.kind).cmp(&format!("{:?}", right.kind)))
    });
    imports.dedup_by(|left, right| left.module == right.module && left.kind == right.kind);
}

/// Collects source trait conformance facts into deterministic CoreIR summaries.
///
/// Inputs:
/// - `module`: compiler-facing syntax output.
///
/// Output:
/// - Sorted, deduplicated Core trait conformance summaries.
///
/// Transformation:
/// - Converts declaration-site `implements` and explicit `impl Trait for Type`
///   blocks into backend-neutral conformance facts while preserving source
///   category and visibility. Struct `includes` is not included because it is
///   struct-to-struct shape inclusion, not trait conformance.
pub(crate) fn core_syntax_trait_conformances(
    module: &SyntaxModuleOutput,
) -> Vec<CoreTraitConformance> {
    let mut conformances = Vec::new();

    for declaration in &module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Type {
                name,
                is_public,
                implements,
                ..
            }
            | SyntaxDeclarationPayload::Struct {
                name,
                is_public,
                implements,
                ..
            } => {
                conformances.extend(implements.iter().map(|trait_ref| CoreTraitConformance {
                    trait_ref: normalize_trait_type_text(&trait_ref.text),
                    for_type: name.clone(),
                    source: CoreTraitConformanceSource::Implements,
                    public: *is_public,
                }));
            }
            _ => {}
        }

        if let SyntaxDeclarationPayload::TraitImpl {
            trait_ref,
            for_type,
            is_public,
            ..
        } = &declaration.payload
        {
            conformances.push(CoreTraitConformance {
                trait_ref: normalize_trait_type_text(&trait_ref.text),
                for_type: normalize_trait_type_text(&for_type.text),
                source: CoreTraitConformanceSource::ExplicitImpl,
                public: *is_public,
            });
        }
    }

    conformances.sort_by(|left, right| {
        left.trait_ref
            .cmp(&right.trait_ref)
            .then_with(|| left.for_type.cmp(&right.for_type))
            .then_with(|| format!("{:?}", left.source).cmp(&format!("{:?}", right.source)))
            .then_with(|| left.public.cmp(&right.public))
    });
    conformances.dedup();
    conformances
}

/// Converts syntax-output import kind into CoreIR import kind.
///
/// Inputs:
/// - `kind`: parser-preserved syntax import kind.
/// - `is_type`: whether the source used `import type`.
///
/// Output:
/// - Matching CoreIR import kind.
///
/// Transformation:
/// - Copies the import family tag while preserving type-only module imports as
///   non-executable CoreIR dependencies.
fn core_import_kind(kind: SyntaxImportKind, is_type: bool) -> CoreImportKind {
    match (kind, is_type) {
        (SyntaxImportKind::Module, true) => CoreImportKind::TypeModule,
        (SyntaxImportKind::Module, false) => CoreImportKind::Module,
        (SyntaxImportKind::File, _) => CoreImportKind::File,
        (SyntaxImportKind::Css, _) => CoreImportKind::Css,
        (SyntaxImportKind::Markdown, _) => CoreImportKind::Markdown,
    }
}

/// Builds a stable CoreIR identity for a syntax import declaration.
///
/// Inputs:
/// - `kind`: syntax import family.
/// - `module_name`: dotted module path for normal imports.
/// - `items`: imported items or asset alias.
/// - `source_path`: asset source path when present.
///
/// Output:
/// - Import identity string used by CoreIR contract text and target validation.
///
/// Transformation:
/// - Keeps module imports keyed by module path and asset imports keyed by
///   `alias<-source` so multiple assets remain distinguishable without reading
///   the filesystem.
fn core_import_identity(
    kind: &SyntaxImportKind,
    module_name: &str,
    items: &[terlan_syntax::SyntaxImportItem],
    source_path: &Option<String>,
) -> String {
    match kind {
        SyntaxImportKind::Module => module_import_identity(module_name, items),
        SyntaxImportKind::File | SyntaxImportKind::Css | SyntaxImportKind::Markdown => {
            let alias = items
                .first()
                .map(|item| item.name.as_str())
                .unwrap_or("<missing-alias>");
            let source = source_path.as_deref().unwrap_or("<missing-source>");
            format!("{alias}<-{source}")
        }
    }
}

/// Builds the CoreIR identity for a source module import.
///
/// Inputs:
/// - `module_name`: parser-preserved import module prefix.
/// - `items`: selected/default import items.
///
/// Output:
/// - Fully qualified module identity used by CoreIR imports.
///
/// Transformation:
/// - Preserves braced selected imports such as `std.core.Option.{Some}` as the
///   provider module `std.core.Option`.
/// - Reconstructs default-export module imports such as `std.data.Json.` from
///   parser parts `std.data` + `Json` when the prefix ends in a lower-case
///   package segment.
fn module_import_identity(module_name: &str, items: &[terlan_syntax::SyntaxImportItem]) -> String {
    let Some(item) = items.first() else {
        return module_name.to_string();
    };
    if items.len() == 1
        && item.as_alias.is_none()
        && module_name
            .rsplit('.')
            .next()
            .and_then(|segment| segment.chars().next())
            .is_some_and(|first| first.is_ascii_lowercase())
    {
        format!("{module_name}.{}", item.name)
    } else {
        module_name.to_string()
    }
}
