//! Explains failed convergence without expanding executable continuation bodies.

use super::*;

pub(in crate::compiler::native_ir) fn profile_gap_reason(
    function_body: &NativeExpr,
    continuations: &[NativeContinuation],
    terminal_profiles: &HashMap<usize, ComposedCallProfile>,
    function_labels: &HashMap<usize, String>,
    unavailable_profiles: &HashMap<usize, String>,
) -> String {
    let mut tails = direct_tail_targets(function_body);
    tails.extend(
        continuations
            .iter()
            .flat_map(|continuation| direct_tail_targets(&continuation.body)),
    );
    tails.sort_unstable();
    tails.dedup();
    if let Some(target) = tails
        .into_iter()
        .find(|target| !terminal_profiles.contains_key(target))
    {
        let label = function_labels
            .get(&target)
            .map_or_else(|| target.to_string(), |label| format!("{target} ({label})"));
        let cause = unavailable_profiles
            .get(&target)
            .map_or(String::new(), |reason| format!(": {reason}"));
        return format!("tail target {label} has no converged suspension profile{cause}");
    }
    if continuations.is_empty() {
        return "lowered body has no continuation records".to_string();
    }
    if continuations.len() > MAX_COMPOSED_CALL_CONTINUATIONS {
        return format!(
            "lowered body has {} continuation records; maximum is {MAX_COMPOSED_CALL_CONTINUATIONS}",
            continuations.len()
        );
    }
    let known = continuations
        .iter()
        .map(|continuation| continuation.id)
        .collect::<HashSet<_>>();
    let mut entries = direct_suspend_ids(function_body);
    entries.sort_unstable();
    entries.dedup();
    if entries.is_empty() {
        return "lowered body has no outward suspension entry".to_string();
    }
    if let Some(entry) = entries.into_iter().find(|entry| !known.contains(entry)) {
        return format!("suspension entry continuation {entry} is absent from the lowered pool");
    }
    ComposedCallProfile::build(function_body, continuations, terminal_profiles, None)
        .err()
        .map(|error| error.to_string())
        .unwrap_or_else(|| {
            "continuation graph is not closed under reachable suspension edges".to_string()
        })
}
