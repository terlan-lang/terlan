//! Interface metadata for source trait conformances.

use super::super::imported_type_refs::collect_syntax_imported_type_refs;
use super::*;

/// Collects public and private trait conformance facts from syntax output.
pub(super) fn collect_syntax_trait_conformances(
    module: &SyntaxModuleOutput,
) -> Vec<TraitConformanceSignature> {
    let mut conformances = Vec::new();
    let imported_type_refs = collect_syntax_imported_type_refs(module);

    for declaration in &module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Type {
                name,
                params,
                implements,
                is_public,
                ..
            } => {
                let for_type = declared_generic_type_text(name, params);
                conformances.extend(
                    implements
                        .iter()
                        .map(|trait_ref| TraitConformanceSignature {
                            trait_ref: qualify_syntax_type_text(
                                &normalize_type_text(&trait_ref.text),
                                &imported_type_refs,
                            ),
                            generic_params: declaration_site_impl_params(params),
                            for_type: qualify_syntax_type_text(
                                &normalize_type_text(&for_type),
                                &imported_type_refs,
                            ),
                            is_negative: false,
                            source: TraitConformanceSource::Implements,
                            public: *is_public,
                        }),
                );
            }
            SyntaxDeclarationPayload::Struct {
                name,
                generic_params,
                implements,
                is_public,
                ..
            } => {
                let for_type = declared_generic_type_text(name, generic_params);
                conformances.extend(
                    implements
                        .iter()
                        .map(|trait_ref| TraitConformanceSignature {
                            trait_ref: qualify_syntax_type_text(
                                &normalize_type_text(&trait_ref.text),
                                &imported_type_refs,
                            ),
                            generic_params: declaration_site_impl_params(generic_params),
                            for_type: qualify_syntax_type_text(
                                &normalize_type_text(&for_type),
                                &imported_type_refs,
                            ),
                            is_negative: false,
                            source: TraitConformanceSource::Implements,
                            public: *is_public,
                        }),
                );
            }
            _ => {}
        }

        if let SyntaxDeclarationPayload::TraitImpl {
            trait_ref,
            generic_params,
            for_type,
            is_negative,
            is_public,
            ..
        } = &declaration.payload
        {
            conformances.push(TraitConformanceSignature {
                trait_ref: qualify_syntax_type_text(
                    &normalize_type_text(&trait_ref.text),
                    &imported_type_refs,
                ),
                generic_params: generic_params.clone(),
                for_type: qualify_syntax_type_text(
                    &normalize_type_text(&for_type.text),
                    &imported_type_refs,
                ),
                is_negative: *is_negative,
                source: TraitConformanceSource::ExplicitImpl,
                public: *is_public,
            });
        }
    }

    conformances.sort();
    conformances.dedup();
    conformances
}

fn declared_generic_type_text(name: &str, params: &[String]) -> String {
    if params.is_empty() {
        return name.to_string();
    }
    let params = params
        .iter()
        .map(|param| {
            param
                .trim()
                .trim_start_matches(['-', '+'])
                .split_once("=>")
                .map(|(subject, _)| subject.trim())
                .unwrap_or_else(|| param.trim().trim_start_matches(['-', '+']))
                .split('[')
                .next()
                .unwrap_or_default()
                .trim()
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{name}[{params}]")
}

fn declaration_site_impl_params(params: &[String]) -> Vec<String> {
    params
        .iter()
        .filter(|param| param.contains("=>"))
        .cloned()
        .collect()
}
