use std::collections::HashMap;
use std::time::Instant;

use super::super::super::{
    call_composition::{DynamicCallProfiles, DynamicCallSignature},
    ComposedCallProfile, NativeExpr,
};

pub(super) fn trace_native_aot(started: Instant, phase: &str, detail: impl std::fmt::Display) {
    if std::env::var_os("TERLAN_NATIVE_AOT_TRACE").is_some() {
        eprintln!(
            "terlc native-aot: {phase}: elapsed={}ms {detail}",
            started.elapsed().as_millis()
        );
    }
}

pub(super) fn profile_widths(profiles: &HashMap<usize, ComposedCallProfile>) -> (usize, usize) {
    profiles.values().fold((0, 0), |(total, maximum), profile| {
        (
            total.saturating_add(profile.continuations.len()),
            maximum.max(profile.continuations.len()),
        )
    })
}

pub(super) fn widest_profile_labels(
    profiles: &HashMap<usize, ComposedCallProfile>,
    labels: &HashMap<usize, String>,
) -> String {
    let mut widths = profiles
        .iter()
        .map(|(owner, profile)| {
            (
                profile.continuations.len(),
                labels
                    .get(owner)
                    .cloned()
                    .unwrap_or_else(|| owner.to_string()),
            )
        })
        .collect::<Vec<_>>();
    widths.sort_by(|left, right| right.cmp(left));
    widths
        .into_iter()
        .take(12)
        .map(|(width, label)| format!("{label}={width}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Resolves a tail dynamic call to the merged profile of its closed-world
/// compatible targets. Pure targets intentionally produce an empty profile.
pub(super) fn forwarded_dynamic_profile(
    body: &NativeExpr,
    profiles: &DynamicCallProfiles,
) -> Option<ComposedCallProfile> {
    let (parameters, result) = match body {
        NativeExpr::InvokeClosure {
            parameter_types,
            result_type,
            ..
        }
        | NativeExpr::InvokeClosureThen {
            parameter_types,
            result_type,
            ..
        } => (parameter_types, *result_type),
        NativeExpr::Let { body, .. } => return forwarded_dynamic_profile(body, profiles),
        _ => return None,
    };
    let targets = profiles.get(&DynamicCallSignature {
        parameters: parameters.clone(),
        result,
    })?;
    let mut targets = targets.iter();
    let mut merged = targets.next()?.profile.clone();
    for target in targets {
        merged.merge_recursive_component_profile(&target.profile);
    }
    Some(merged)
}
