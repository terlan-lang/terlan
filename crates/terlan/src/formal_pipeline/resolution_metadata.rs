//! Deterministic serialization of the authoritative resolver's import bindings.

use crate::terlan_hir::{ModuleInterface, ResolvedModule};

/// Existing checked interface classification, not a new effect inference pass.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub(crate) struct PurityContractMetadata {
    module: String,
    name: String,
    arity: usize,
    parameter_types: Vec<String>,
    return_type: String,
    generic_params: Vec<String>,
    generic_bounds: Vec<String>,
    receiver_method: bool,
    receiver_mutable: bool,
    public: bool,
    compiler_proven_pure: bool,
}

pub(crate) fn purity_contracts(interface: &ModuleInterface) -> Vec<PurityContractMetadata> {
    let mut rows = interface
        .functions
        .values()
        .chain(interface.function_overloads.values().flatten())
        .map(|signature| PurityContractMetadata {
            module: interface.module.clone(),
            name: signature.name.clone(),
            arity: signature.params.len(),
            parameter_types: signature
                .params
                .iter()
                .map(|parameter| parameter.annotation.clone())
                .collect(),
            return_type: signature.return_type.clone(),
            generic_params: signature.generic_params.clone(),
            generic_bounds: signature.generic_bounds.clone(),
            receiver_method: signature.receiver_method,
            receiver_mutable: signature.receiver_mutable,
            public: signature.public,
            compiler_proven_pure: signature.pure,
        })
        .collect::<Vec<_>>();
    rows.sort();
    rows.dedup();
    rows
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub(crate) struct ResolvedImportMetadata {
    pub(crate) kind: &'static str,
    pub(crate) local_name: String,
    pub(crate) source_module: String,
    pub(crate) source_name: String,
    pub(crate) span_start: usize,
    pub(crate) span_end: usize,
}

pub(super) fn imports(resolved: &ResolvedModule) -> Vec<ResolvedImportMetadata> {
    let mut rows = Vec::new();
    for (kind, imported) in [
        ("type", &resolved.imported_types),
        ("trait", &resolved.imported_traits),
        ("constant", &resolved.imported_constants),
    ] {
        rows.extend(imported.values().map(|item| ResolvedImportMetadata {
            kind,
            local_name: item.local_name.clone(),
            source_module: item.source_module.clone(),
            source_name: item.source_name.clone(),
            span_start: item.span.start,
            span_end: item.span.end,
        }));
    }
    rows.sort();
    rows.dedup();
    rows
}

/// Includes explicit negative facts discarded by executable Core conformance lists.
pub(crate) fn conformance_contracts(interface: &ModuleInterface) -> Vec<serde_json::Value> {
    let mut facts = interface.trait_conformances.clone();
    facts.sort();
    facts.dedup();
    facts.into_iter().map(|fact| serde_json::json!({
        "module": interface.module, "trait_ref": fact.trait_ref, "for_type": fact.for_type,
        "generic_params": fact.generic_params, "is_negative": fact.is_negative, "public": fact.public,
        "source": match fact.source {
            crate::terlan_hir::TraitConformanceSource::Implements => "implements",
            crate::terlan_hir::TraitConformanceSource::ExplicitImpl => "explicit_impl",
        }
    })).collect()
}
