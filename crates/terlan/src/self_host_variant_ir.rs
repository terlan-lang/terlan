//! ABI decoder and validator for self-hosted finite-sum runtime layouts.

/// Failure to resolve constructor metadata for one concrete finite-sum type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VariantResolutionError(String);

impl std::fmt::Display for VariantResolutionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for VariantResolutionError {}
impl std::ops::Deref for VariantResolutionError {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}
impl From<String> for VariantResolutionError {
    fn from(message: String) -> Self {
        Self(message)
    }
}
impl From<&str> for VariantResolutionError {
    fn from(message: &str) -> Self {
        Self(message.to_owned())
    }
}
impl From<VariantResolutionError> for String {
    fn from(error: VariantResolutionError) -> Self {
        error.0
    }
}

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const VARIANT_LAYOUT_SCHEMA: &str = "terlan.self-host.variant-layout/v1";

/// Serialized payload type evidence; an unknown type must have an empty name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionalType {
    pub known: bool,
    pub name: String,
}

/// Serialized constructor tag, generic parameters, and ordered payload types.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariantLayout {
    pub owner: String,
    pub name: String,
    pub discriminant: u32,
    pub type_parameters: Vec<String>,
    pub payload_types: Vec<OptionalType>,
}

/// Versioned finite-sum sidecar containing the layouts of its constructors.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VariantLayoutDocument {
    pub schema: String,
    pub variants: Vec<VariantLayout>,
}

fn validate_optional_type(
    owner: &str,
    constructor: &str,
    index: usize,
    value: &OptionalType,
    diagnostics: &mut Vec<String>,
) {
    if value.known && value.name.trim().is_empty() {
        diagnostics.push(format!(
            "variant {owner}.{constructor} payload {index} is marked known without a type"
        ));
    }
    if !value.known && !value.name.is_empty() {
        diagnostics.push(format!(
            "variant {owner}.{constructor} payload {index} is marked unknown but carries a type"
        ));
    }
}

/// Returns deterministic diagnostics for an already-decoded sidecar.
pub fn validate(document: &VariantLayoutDocument) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if document.schema != VARIANT_LAYOUT_SCHEMA {
        diagnostics.push(format!(
            "unsupported variant-layout schema `{}`; expected `{VARIANT_LAYOUT_SCHEMA}`",
            document.schema
        ));
    }

    let mut names = BTreeSet::new();
    let mut tags = BTreeSet::new();
    let mut next_tags = BTreeMap::<String, u32>::new();
    let mut closed_owners = BTreeSet::new();
    let mut current_owner: Option<&str> = None;

    for variant in &document.variants {
        let owner = variant.owner.trim();
        let name = variant.name.trim();
        if owner.is_empty() {
            diagnostics.push("variant owner must not be empty".to_owned());
        }
        if name.is_empty() {
            diagnostics.push(format!(
                "variant in `{owner}` has an empty constructor name"
            ));
        }

        if current_owner != Some(owner) {
            if let Some(previous) = current_owner {
                closed_owners.insert(previous.to_owned());
            }
            if closed_owners.contains(owner) {
                diagnostics.push(format!(
                    "variant owner `{owner}` is split across non-contiguous groups"
                ));
            }
            current_owner = Some(owner);
        }

        if !names.insert((owner.to_owned(), name.to_owned())) {
            diagnostics.push(format!("duplicate variant constructor `{owner}.{name}`"));
        }
        if !tags.insert((owner.to_owned(), variant.discriminant)) {
            diagnostics.push(format!(
                "duplicate discriminant {} for variant owner `{owner}`",
                variant.discriminant
            ));
        }

        let expected = next_tags.entry(owner.to_owned()).or_insert(0);
        if variant.discriminant != *expected {
            diagnostics.push(format!(
                "variant `{owner}.{name}` has discriminant {}; expected {}",
                variant.discriminant, *expected
            ));
        }
        *expected = expected.saturating_add(1);

        let mut parameters = BTreeSet::new();
        for parameter in &variant.type_parameters {
            let parameter = parameter.trim();
            if parameter.is_empty() {
                diagnostics.push(format!(
                    "variant `{owner}.{name}` has an empty type parameter"
                ));
            } else if !parameters.insert(parameter.to_owned()) {
                diagnostics.push(format!(
                    "variant `{owner}.{name}` repeats type parameter `{parameter}`"
                ));
            }
        }

        for (index, payload) in variant.payload_types.iter().enumerate() {
            validate_optional_type(owner, name, index, payload, &mut diagnostics);
        }
    }

    diagnostics
}

/// Decodes and validates the exact self-hosted variant-layout ABI.
pub fn decode(source: &str) -> Result<VariantLayoutDocument, Vec<String>> {
    let document: VariantLayoutDocument = serde_json::from_str(source)
        .map_err(|error| vec![format!("invalid variant-layout JSON: {error}")])?;
    let diagnostics = validate(&document);
    if diagnostics.is_empty() {
        Ok(document)
    } else {
        Err(diagnostics)
    }
}

/// Constructor layout after substituting a concrete subject's type arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedVariantLayout {
    pub owner: String,
    pub name: String,
    pub discriminant: u32,
    pub variant_count: u32,
    pub payload_types: Vec<Option<String>>,
}

/// Validated constructor layouts indexed by their owner and constructor names.
#[derive(Clone, Debug, Default)]
pub struct VariantCatalog {
    layouts: BTreeMap<(String, String), VariantLayout>,
}

fn generic_application(value: &str) -> Option<(&str, Vec<&str>)> {
    let value = value.trim();
    let opening = value.find('[')?;
    if !value.ends_with(']') {
        return None;
    }
    let inner = &value[opening + 1..value.len() - 1];
    let mut arguments = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, character) in inner.char_indices() {
        match character {
            '[' | '(' | '{' => depth = depth.saturating_add(1),
            ']' | ')' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                arguments.push(inner[start..index].trim());
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    if !inner.trim().is_empty() {
        arguments.push(inner[start..].trim());
    }
    Some((value[..opening].trim(), arguments))
}

fn unqualified(value: &str) -> &str {
    value.rsplit('.').next().unwrap_or(value)
}

fn substitute_type(value: &str, bindings: &BTreeMap<String, String>) -> String {
    let value = value.trim();
    if let Some(replacement) = bindings.get(value) {
        return replacement.clone();
    }
    let Some((base, arguments)) = generic_application(value) else {
        return value.to_owned();
    };
    let arguments = arguments
        .into_iter()
        .map(|argument| substitute_type(argument, bindings))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{base}[{arguments}]")
}

impl VariantCatalog {
    /// Validates a sidecar and indexes its layouts, returning any admission errors.
    pub fn from_document(document: VariantLayoutDocument) -> Result<Self, Vec<String>> {
        let diagnostics = validate(&document);
        if !diagnostics.is_empty() {
            return Err(diagnostics);
        }
        let layouts = document
            .variants
            .into_iter()
            .map(|layout| ((layout.owner.clone(), layout.name.clone()), layout))
            .collect();
        Ok(Self { layouts })
    }

    /// Merges per-module catalogs into one application authority.
    /// Identical imported layouts may repeat, while conflicting identities fail closed.
    pub fn merge(catalogs: impl IntoIterator<Item = Self>) -> Result<Self, Vec<String>> {
        let mut layouts = BTreeMap::new();
        let mut diagnostics = Vec::new();
        for catalog in catalogs {
            for (identity, layout) in catalog.layouts {
                if let Some(previous) = layouts.get(&identity) {
                    if previous != &layout {
                        diagnostics.push(format!(
                            "conflicting variant layout `{}.{}` across application modules",
                            identity.0, identity.1
                        ));
                    }
                    continue;
                }
                layouts.insert(identity, layout);
            }
        }
        if diagnostics.is_empty() {
            Ok(Self { layouts })
        } else {
            Err(diagnostics)
        }
    }

    /// Resolves a constructor against a subject type and substitutes generic payloads.
    /// Rejects unknown constructors, owner mismatches, and invalid type-argument counts.
    pub fn resolve(
        &self,
        owner: &str,
        constructor: &str,
        subject_type: &str,
    ) -> Result<ResolvedVariantLayout, VariantResolutionError> {
        let layout = self
            .layouts
            .get(&(owner.to_owned(), constructor.to_owned()))
            .ok_or_else(|| format!("unknown variant constructor `{owner}.{constructor}`"))?;
        let subject_type = subject_type.trim();
        let (subject_base, subject_arguments) =
            generic_application(subject_type).unwrap_or((subject_type, Vec::new()));
        if unqualified(subject_base) != unqualified(owner) {
            return Err((format!(
                "variant `{owner}.{constructor}` cannot match subject type `{subject_type}`"
            ))
            .into());
        }
        if !layout.type_parameters.is_empty()
            && layout.type_parameters.len() != subject_arguments.len()
        {
            return Err((format!(
                "variant `{owner}.{constructor}` expects {} type arguments but subject `{subject_type}` supplies {}",
                layout.type_parameters.len(),
                subject_arguments.len()
            )).into());
        }

        let bindings = layout
            .type_parameters
            .iter()
            .cloned()
            .zip(subject_arguments.into_iter().map(str::to_owned))
            .collect::<BTreeMap<_, _>>();
        let payload_types = layout
            .payload_types
            .iter()
            .map(|payload| {
                if payload.known {
                    Some(substitute_type(&payload.name, &bindings))
                } else {
                    None
                }
            })
            .collect();
        let variant_count = u32::try_from(
            self.layouts
                .values()
                .filter(|candidate| candidate.owner == layout.owner)
                .count(),
        )
        .map_err(|_| format!("variant owner `{owner}` exceeds the runtime variant-count limit"))?;
        if variant_count == 0 {
            return Err((format!("variant owner `{owner}` has no constructors")).into());
        }

        Ok(ResolvedVariantLayout {
            owner: layout.owner.clone(),
            name: layout.name.clone(),
            discriminant: layout.discriminant,
            variant_count,
            payload_types,
        })
    }
}

/// Decodes a sidecar directly into backend-ready variant lookup authority.
pub fn decode_catalog(source: &str) -> Result<VariantCatalog, Vec<String>> {
    decode(source).and_then(VariantCatalog::from_document)
}

#[cfg(test)]
#[path = "self_host_variant_ir_inline_test.rs"]
mod tests;
