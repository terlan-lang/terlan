//! Validation of converged native suspension profiles.

use super::*;

/// Requires emission to preserve the converged continuation identities and bodies.
pub(super) fn validate_emitted(
    candidate: &Candidate<'_>,
    expected: &ComposedCallProfile,
    emitted: &ComposedCallProfile,
) -> super::super::super::NativeIrResult<()> {
    if emitted == expected {
        return Ok(());
    }
    let expected_ids = expected
        .continuations
        .iter()
        .map(|c| c.id)
        .collect::<HashSet<_>>();
    let emitted_ids = emitted
        .continuations
        .iter()
        .map(|c| c.id)
        .collect::<HashSet<_>>();
    let mut missing = expected_ids
        .difference(&emitted_ids)
        .copied()
        .collect::<Vec<_>>();
    let mut extra = emitted_ids
        .difference(&expected_ids)
        .copied()
        .collect::<Vec<_>>();
    missing.sort_unstable();
    extra.sort_unstable();
    Err(format!(
        "error[native_ir.profile_emission]: final lowering for `{}.{}/{}` differs from its converged suspension profile; missing={missing:?}, extra={extra:?}",
        candidate.core.module, candidate.function.name, candidate.function.arity
    ).into())
}

/// Checks each continuation once against the final application profiles.
pub(super) fn validate_profiles(
    started: Instant,
    call_profiles: &HashMap<usize, ComposedCallProfile>,
    native_function_labels: &HashMap<usize, String>,
) -> super::super::super::NativeIrResult<()> {
    let mut profile_owners = call_profiles.keys().copied().collect::<Vec<_>>();
    profile_owners.sort_unstable();
    let profile_destination_capture_counts = call_profiles
        .values()
        .flat_map(|profile| {
            profile
                .continuations
                .iter()
                .map(|continuation| (continuation.id, continuation.params.len()))
        })
        .collect::<HashMap<_, _>>();
    let mut validated_profile_continuations = HashSet::new();
    trace_native_aot(
        started,
        "profile-contracts-start",
        format_args!("profiles={}", profile_owners.len()),
    );
    for owner in profile_owners {
        let profile = &call_profiles[&owner];
        for continuation in &profile.continuations {
            if !validated_profile_continuations.insert(continuation.id) {
                continue;
            }
            super::super::super::call_composition::validate_call_then_contracts_with_destinations(
                &continuation.body,
                call_profiles,
                native_function_labels,
                &profile_destination_capture_counts,
            )
            .map_err(|error| {
                let owner = native_function_labels
                    .get(&owner)
                    .map_or_else(|| owner.to_string(), Clone::clone);
                format!(
                    "error[native_ir.call_profile_contract]: {error}; in continuation {} of `{owner}`",
                    continuation.id
                )
            })?;
        }
    }
    trace_native_aot(
        started,
        "profile-contracts-complete",
        format_args!(
            "unique-continuations={}",
            validated_profile_continuations.len()
        ),
    );
    Ok(())
}
