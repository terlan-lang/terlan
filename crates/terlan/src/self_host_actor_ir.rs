//! Decoder and identity authority for self-hosted actor continuation Core IR.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const ACTOR_IR_SCHEMA: &str = "terlan.self-host.actor-ir/v2";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorContinuation {
    pub id: String,
    pub source_module: String,
    pub source_function: String,
    pub source_arity: u64,
    pub identity_kind: String,
    pub outward_entry: bool,
    pub has_ordinal: bool,
    pub ordinal: u64,
    pub capture_names: Vec<String>,
    pub capture_types: Vec<String>,
    pub parameter_names: Vec<String>,
    pub parameter_types: Vec<String>,
    pub return_type: String,
    pub entry_block: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorResumeEdge {
    pub has_callee_export_id: bool,
    pub callee_export_id: String,
    pub callee_continuation_id: String,
    pub callee_capture_count: usize,
    pub continuation_id: String,
    pub caller_value_start: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorSuspensionSite {
    pub source_function: String,
    pub source_arity: usize,
    pub native_call_index: usize,
    pub block_label: String,
    pub result_name: String,
    pub callee_module: String,
    pub callee_function: String,
    pub callee_arity: usize,
    pub value_names: Vec<String>,
    pub resumes: Vec<ActorResumeEdge>,
    pub completion_continuation_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorProfileEntry {
    pub has_export_identity: bool,
    pub export_identity: String,
    pub continuation_identity: String,
    pub capture_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorSuspensionProfile {
    pub module_name: String,
    pub callable_name: String,
    pub arity: usize,
    pub runtime_owned: bool,
    pub entries: Vec<ActorProfileEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorIrDocument {
    pub schema: String,
    pub continuations: Vec<ActorContinuation>,
    pub suspension_sites: Vec<ActorSuspensionSite>,
    pub profiles: Vec<ActorSuspensionProfile>,
}

fn identity_domain(kind: &str) -> Option<(&'static [u8], bool)> {
    match kind {
        "continuation" => Some((b"terlan-tvm-continuation-v1\0", true)),
        "composed-completion" => Some((b"terlan-tvm-composed-completion-v1\0", true)),
        "reduction-continuation" => Some((b"terlan-tvm-reduction-continuation-v1\0", false)),
        "reduction-completion" => Some((b"terlan-tvm-reduction-completion-v1\0", false)),
        _ => None,
    }
}

fn expected_identity(value: &ActorContinuation) -> Result<String, String> {
    let (domain, needs_ordinal) = identity_domain(&value.identity_kind).ok_or_else(|| {
        format!(
            "unknown continuation identity kind `{}`",
            value.identity_kind
        )
    })?;
    if value.has_ordinal != needs_ordinal || (!value.has_ordinal && value.ordinal != 0) {
        return Err(format!(
            "continuation `{}` has identity provenance inconsistent with `{}`",
            value.id, value.identity_kind
        ));
    }
    let mut digest = Sha256::new();
    digest.update(domain);
    digest.update(value.source_module.as_bytes());
    digest.update(b"\0");
    digest.update(value.source_function.as_bytes());
    digest.update(b"\0");
    digest.update(value.source_arity.to_le_bytes());
    if value.has_ordinal {
        digest.update(value.ordinal.to_le_bytes());
    }
    let hex = digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("sha256:{hex}"))
}

/// Converts one verified full digest identity to the native nonzero dispatch ID.
pub fn dispatch_id(identity: &str) -> Result<u64, String> {
    let hex = identity
        .strip_prefix("sha256:")
        .ok_or_else(|| "continuation identity is missing `sha256:` prefix".to_owned())?;
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(
            "continuation identity must contain 64 lowercase hexadecimal digits".to_owned(),
        );
    }
    let mut prefix = [0u8; 8];
    for (index, byte) in prefix.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
            .map_err(|_| "continuation identity contains invalid hexadecimal".to_owned())?;
    }
    Ok(u64::from_le_bytes(prefix).max(1))
}

pub fn validate(document: &ActorIrDocument) -> Vec<String> {
    let mut diagnostics = Vec::new();
    if document.schema != ACTOR_IR_SCHEMA {
        diagnostics.push(format!(
            "unsupported actor IR schema `{}`; expected `{ACTOR_IR_SCHEMA}`",
            document.schema
        ));
    }
    let mut profile_identities = BTreeSet::new();
    for profile in &document.profiles {
        if profile.module_name.is_empty() || profile.callable_name.is_empty() {
            diagnostics.push("actor suspension profile has an empty callable identity".to_owned());
        }
        if !profile_identities.insert((
            profile.module_name.clone(),
            profile.callable_name.clone(),
            profile.arity,
        )) {
            diagnostics.push(format!(
                "duplicate actor suspension profile `{}.{}/{}`",
                profile.module_name, profile.callable_name, profile.arity
            ));
        }
        if profile.entries.is_empty() {
            diagnostics.push(format!(
                "actor suspension profile `{}.{}/{}` has no entries",
                profile.module_name, profile.callable_name, profile.arity
            ));
        }
        let mut entries = BTreeSet::new();
        for entry in &profile.entries {
            if entry.has_export_identity != !entry.export_identity.is_empty() {
                diagnostics.push(format!(
                    "actor suspension profile `{}.{}/{}` has inconsistent export identity presence",
                    profile.module_name, profile.callable_name, profile.arity
                ));
            }
            if entry.has_export_identity && dispatch_id(&entry.export_identity).is_err() {
                diagnostics.push(format!(
                    "actor suspension profile `{}.{}/{}` has invalid export identity `{}`",
                    profile.module_name,
                    profile.callable_name,
                    profile.arity,
                    entry.export_identity
                ));
            }
            if dispatch_id(&entry.continuation_identity).is_err() {
                diagnostics.push(format!(
                    "actor suspension profile `{}.{}/{}` has invalid continuation identity `{}`",
                    profile.module_name,
                    profile.callable_name,
                    profile.arity,
                    entry.continuation_identity
                ));
            }
            if !entries.insert((
                entry.has_export_identity,
                entry.export_identity.clone(),
                entry.continuation_identity.clone(),
                entry.capture_count,
            )) {
                diagnostics.push(format!(
                    "actor suspension profile `{}.{}/{}` has duplicate entries",
                    profile.module_name, profile.callable_name, profile.arity
                ));
            }
        }
    }
    let mut ids = BTreeSet::new();
    let mut dispatch_ids = BTreeSet::new();
    for continuation in &document.continuations {
        if !ids.insert(continuation.id.clone()) {
            diagnostics.push(format!(
                "duplicate continuation identity `{}`",
                continuation.id
            ));
        }
        match expected_identity(continuation) {
            Ok(expected) if expected != continuation.id => diagnostics.push(format!(
                "continuation identity mismatch for `{}.{}/{}`: expected `{expected}`, found `{}`",
                continuation.source_module,
                continuation.source_function,
                continuation.source_arity,
                continuation.id
            )),
            Err(error) => diagnostics.push(error),
            _ => {}
        }
        if let Ok(id) = dispatch_id(&continuation.id) {
            if !dispatch_ids.insert(id) {
                diagnostics.push(format!(
                    "continuation identity `{}` collides after native dispatch truncation",
                    continuation.id
                ));
            }
        }
        if continuation.capture_names.len() != continuation.capture_types.len() {
            diagnostics.push(format!(
                "continuation `{}` has mismatched capture names and types",
                continuation.id
            ));
        }
        if continuation.parameter_types.len() < continuation.capture_types.len()
            || continuation.parameter_types[..continuation.capture_types.len()]
                != continuation.capture_types
        {
            diagnostics.push(format!(
                "continuation `{}` parameters do not begin with its ordered capture types",
                continuation.id
            ));
        }
        if continuation.parameter_names.len() != continuation.parameter_types.len()
            || continuation.parameter_names.len() < continuation.capture_names.len()
            || continuation.parameter_names[..continuation.capture_names.len()]
                != continuation.capture_names
        {
            diagnostics.push(format!(
                "continuation `{}` parameter names do not preserve its ordered captures",
                continuation.id
            ));
        }
        if continuation.source_module.is_empty()
            || continuation.source_function.is_empty()
            || continuation.entry_block.is_empty()
            || continuation.return_type.is_empty()
        {
            diagnostics.push(format!(
                "continuation `{}` has incomplete ownership or type metadata",
                continuation.id
            ));
        }
    }
    for profile in &document.profiles {
        for entry in &profile.entries {
            if entry.has_export_identity {
                diagnostics.push(format!(
                    "local actor suspension profile `{}.{}/{}` entry unexpectedly has an export identity",
                    profile.module_name, profile.callable_name, profile.arity
                ));
            }
            if profile.runtime_owned {
                continue;
            }
            match document
                .continuations
                .iter()
                .find(|continuation| continuation.id == entry.continuation_identity)
            {
                None => diagnostics.push(format!(
                    "actor suspension profile `{}.{}/{}` references unknown continuation `{}`",
                    profile.module_name,
                    profile.callable_name,
                    profile.arity,
                    entry.continuation_identity
                )),
                Some(continuation)
                    if !continuation.outward_entry
                        || continuation.source_module != profile.module_name
                        || continuation.source_function != profile.callable_name
                        || continuation.source_arity as usize != profile.arity
                        || continuation.capture_names.len() != entry.capture_count =>
                {
                    diagnostics.push(format!(
                        "actor suspension profile `{}.{}/{}` continuation `{}` has incompatible ownership or capture layout",
                        profile.module_name,
                        profile.callable_name,
                        profile.arity,
                        entry.continuation_identity
                    ));
                }
                Some(_) => {}
            }
        }
    }
    for continuation in document
        .continuations
        .iter()
        .filter(|continuation| continuation.outward_entry)
    {
        let covered = document.profiles.iter().any(|profile| {
            profile.module_name == continuation.source_module
                && profile.callable_name == continuation.source_function
                && profile.arity == continuation.source_arity as usize
                && profile.entries.iter().any(|entry| {
                    !entry.has_export_identity
                        && entry.continuation_identity == continuation.id
                        && entry.capture_count == continuation.capture_names.len()
                })
        });
        if !covered {
            diagnostics.push(format!(
                "outward continuation `{}` is absent from its owning suspension profile",
                continuation.id
            ));
        }
    }
    for site in &document.suspension_sites {
        if site.source_function.is_empty()
            || site.block_label.is_empty()
            || site.callee_module.is_empty()
            || site.callee_function.is_empty()
        {
            diagnostics.push("suspension site has incomplete owner or callee identity".to_owned());
        }
        if !ids.contains(&site.completion_continuation_id) {
            diagnostics.push(format!(
                "suspension site references unknown completion `{}`",
                site.completion_continuation_id
            ));
        }
        for resume in &site.resumes {
            if !ids.contains(&resume.continuation_id) {
                diagnostics.push(format!(
                    "resume edge references unknown continuation `{}`",
                    resume.continuation_id
                ));
            }
            if dispatch_id(&resume.callee_continuation_id).is_err() {
                diagnostics.push(format!(
                    "resume edge has malformed callee continuation `{}`",
                    resume.callee_continuation_id
                ));
            }
            if resume.has_callee_export_id != !resume.callee_export_id.is_empty()
                || (resume.has_callee_export_id && dispatch_id(&resume.callee_export_id).is_err())
            {
                diagnostics.push(format!(
                    "resume edge has malformed callee export identity `{}`",
                    resume.callee_export_id
                ));
            }
            if resume.caller_value_start > site.value_names.len() {
                diagnostics.push(format!(
                    "resume edge in `{}:{}` has an invalid caller value offset",
                    site.source_function, site.block_label
                ));
            }
        }
        if site
            .resumes
            .iter()
            .all(|resume| !resume.has_callee_export_id)
        {
            if let Some(profile) = document.profiles.iter().find(|profile| {
                profile.module_name == site.callee_module
                    && profile.callable_name == site.callee_function
                    && profile.arity == site.callee_arity
            }) {
                let expected = profile
                    .entries
                    .iter()
                    .map(|entry| {
                        (
                            entry.has_export_identity,
                            entry.export_identity.as_str(),
                            entry.continuation_identity.as_str(),
                            entry.capture_count,
                        )
                    })
                    .collect::<BTreeSet<_>>();
                let actual = site
                    .resumes
                    .iter()
                    .map(|resume| {
                        (
                            resume.has_callee_export_id,
                            resume.callee_export_id.as_str(),
                            resume.callee_continuation_id.as_str(),
                            resume.callee_capture_count,
                        )
                    })
                    .collect::<BTreeSet<_>>();
                if expected.len() != profile.entries.len()
                    || actual.len() != site.resumes.len()
                    || expected != actual
                {
                    diagnostics.push(format!(
                        "suspension site `{}:{}` does not exactly match local profile `{}.{}/{}`",
                        site.source_function,
                        site.block_label,
                        profile.module_name,
                        profile.callable_name,
                        profile.arity
                    ));
                }
            }
        }
    }
    diagnostics
}

pub fn decode(source: &str) -> Result<ActorIrDocument, Vec<String>> {
    let document: ActorIrDocument = serde_json::from_str(source)
        .map_err(|error| vec![format!("invalid actor IR JSON: {error}")])?;
    let diagnostics = validate(&document);
    if diagnostics.is_empty() {
        Ok(document)
    } else {
        Err(diagnostics)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeActorContinuation {
    pub id: u64,
    pub source_module: String,
    pub source_function: String,
    pub source_arity: usize,
    pub capture_names: Vec<String>,
    pub capture_types: Vec<String>,
    pub parameter_names: Vec<String>,
    pub parameter_types: Vec<String>,
    pub return_type: String,
    pub entry_block: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeActorResumeEdge {
    pub callee_export_id: Option<u64>,
    pub callee_continuation_id: u64,
    pub callee_capture_count: usize,
    pub continuation_id: u64,
    pub caller_value_start: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeActorSuspensionSite {
    pub source_function: String,
    pub source_arity: usize,
    pub native_call_index: usize,
    pub block_label: String,
    pub result_name: String,
    pub callee_module: String,
    pub callee_function: String,
    pub callee_arity: usize,
    pub value_names: Vec<String>,
    pub resumes: Vec<RuntimeActorResumeEdge>,
    pub completion_continuation_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeActorPlan {
    pub continuations: Vec<RuntimeActorContinuation>,
    pub suspension_sites: Vec<RuntimeActorSuspensionSite>,
}

fn materialize_continuation(value: ActorContinuation) -> Result<RuntimeActorContinuation, String> {
    Ok(RuntimeActorContinuation {
        id: dispatch_id(&value.id)?,
        source_module: value.source_module,
        source_function: value.source_function,
        source_arity: usize::try_from(value.source_arity)
            .map_err(|_| "continuation source arity exceeds the native target".to_owned())?,
        capture_names: value.capture_names,
        capture_types: value.capture_types,
        parameter_names: value.parameter_names,
        parameter_types: value.parameter_types,
        return_type: value.return_type,
        entry_block: value.entry_block,
    })
}

fn materialize_resume(value: ActorResumeEdge) -> Result<RuntimeActorResumeEdge, String> {
    Ok(RuntimeActorResumeEdge {
        callee_export_id: if value.has_callee_export_id {
            Some(dispatch_id(&value.callee_export_id)?)
        } else {
            None
        },
        callee_continuation_id: dispatch_id(&value.callee_continuation_id)?,
        callee_capture_count: value.callee_capture_count,
        continuation_id: dispatch_id(&value.continuation_id)?,
        caller_value_start: value.caller_value_start,
    })
}

fn materialize_site(value: ActorSuspensionSite) -> Result<RuntimeActorSuspensionSite, String> {
    let resumes = value
        .resumes
        .into_iter()
        .map(materialize_resume)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RuntimeActorSuspensionSite {
        source_function: value.source_function,
        source_arity: value.source_arity,
        native_call_index: value.native_call_index,
        block_label: value.block_label,
        result_name: value.result_name,
        callee_module: value.callee_module,
        callee_function: value.callee_function,
        callee_arity: value.callee_arity,
        value_names: value.value_names,
        resumes,
        completion_continuation_id: dispatch_id(&value.completion_continuation_id)?,
    })
}

/// Converts validated symbolic actor IR into native-width scheduler metadata.
pub fn materialize(document: ActorIrDocument) -> Result<RuntimeActorPlan, Vec<String>> {
    let diagnostics = validate(&document);
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    let continuations = document
        .continuations
        .into_iter()
        .map(materialize_continuation)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| vec![error])?;
    let suspension_sites = document
        .suspension_sites
        .into_iter()
        .map(materialize_site)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| vec![error])?;
    Ok(RuntimeActorPlan {
        continuations,
        suspension_sites,
    })
}

/// Decodes, verifies, and materializes actor IR in one backend admission step.
pub fn decode_runtime_plan(source: &str) -> Result<RuntimeActorPlan, Vec<String>> {
    decode(source).and_then(materialize)
}
