//! Stable native test-planning facts for bounded performance diagnostics.

use std::path::PathBuf;
use std::time::Instant;

use crate::terlan_typeck::CoreModule;

pub(super) fn emit(enabled: bool, cores: &[CoreModule], providers: &[(String, PathBuf)]) {
    let Some(line) = facts_line(enabled, reachable_declarations(cores), providers) else {
        return;
    };
    eprintln!("{line}");
}

pub(super) fn timed<R>(enabled: bool, phase: &str, operation: impl FnOnce() -> R) -> R {
    let started = Instant::now();
    let result = operation();
    if let Some(line) = phase_line(enabled, phase, started.elapsed().as_millis()) {
        eprintln!("{line}");
    }
    result
}

fn phase_line(enabled: bool, phase: &str, elapsed_ms: u128) -> Option<String> {
    enabled.then(|| format!("terlc timing: test.{phase}: +{elapsed_ms}ms total={elapsed_ms}ms"))
}

fn reachable_declarations(cores: &[CoreModule]) -> usize {
    cores
        .iter()
        .map(|core| {
            declaration_count([
                core.functions.len(),
                core.constructors.len(),
                core.types.len(),
                core.templates.len(),
                core.trait_conformances.len(),
            ])
        })
        .sum()
}

fn declaration_count(counts: [usize; 5]) -> usize {
    counts.into_iter().sum()
}

fn facts_line(
    enabled: bool,
    reachable_declarations: usize,
    providers: &[(String, PathBuf)],
) -> Option<String> {
    if !enabled {
        return None;
    }
    let mut providers = providers
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    providers.sort_unstable();
    providers.dedup();
    let providers = if providers.is_empty() {
        "none".to_string()
    } else {
        providers.join(",")
    };
    Some(format!(
        "terlc native-aot: planning.graph: reachable-declarations={reachable_declarations} native-providers={providers}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planning_facts_are_bounded_sorted_and_optional() {
        let providers = vec![
            ("TERLAN_Z".to_string(), PathBuf::from("z")),
            ("TERLAN_A".to_string(), PathBuf::from("a")),
            ("TERLAN_A".to_string(), PathBuf::from("duplicate")),
        ];
        assert_eq!(declaration_count([1, 2, 3, 4, 5]), 15);
        assert_eq!(reachable_declarations(&[]), 0);
        emit(false, &[], &[]);
        emit(true, &[], &[]);
        assert_eq!(facts_line(false, 15, &providers), None);
        assert_eq!(
            facts_line(true, 15, &providers).as_deref(),
            Some(
                "terlc native-aot: planning.graph: reachable-declarations=15 native-providers=TERLAN_A,TERLAN_Z"
            )
        );
        assert_eq!(
            facts_line(true, 0, &[]).as_deref(),
            Some(
                "terlc native-aot: planning.graph: reachable-declarations=0 native-providers=none"
            )
        );
        assert_eq!(phase_line(false, "execution", 7), None);
        assert_eq!(
            phase_line(true, "execution", 7).as_deref(),
            Some("terlc timing: test.execution: +7ms total=7ms")
        );
        assert_eq!(timed(false, "probe", || 11), 11);
        assert_eq!(timed(true, "probe", || 13), 13);
    }
}
