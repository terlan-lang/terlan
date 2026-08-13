use super::{phase_with_feature_filter, test_phases};

#[test]
fn orchestrator_runs_shared_runtime_tests_in_the_library_harness() {
    let phases = test_phases(false);
    let library = phases.first().expect("Terlan library phase");

    assert_eq!(library.name, "Terlan library");
    assert!(library.args.contains(&"--lib"));
    assert!(!library.args.contains(&"--bin"));
}

#[test]
fn orchestrator_uses_owned_namespace_filters_for_embedded_harnesses() {
    let quality = phase_with_feature_filter("quality", "quality-tools", "quality::");

    assert!(quality.args.contains(&"--lib"));
    assert!(quality.args.contains(&"quality::"));
    assert!(!quality.args.contains(&"--bin"));
}

#[test]
fn orchestrator_enables_every_feature_gated_harness() {
    let lsp = phase_with_feature_filter("LSP", "editor-lsp", "lsp::");

    assert!(lsp
        .args
        .windows(2)
        .any(|arguments| arguments == ["--features", "editor-lsp"]));
    for (name, feature) in [
        ("quality", "quality-tools"),
        ("LSP", "editor-lsp"),
        ("benchmark harness", "benchmark-tools"),
    ] {
        let phase = test_phases(false)
            .into_iter()
            .find(|phase| phase.name == name)
            .expect("feature-gated phase");
        assert!(phase
            .args
            .windows(2)
            .any(|arguments| arguments == ["--features", feature]));
    }
    assert!(!test_phases(false)
        .first()
        .expect("Terlan library phase")
        .args
        .contains(&"--features"));
}

#[test]
fn orchestrator_runs_ignored_contract_once_in_the_library() {
    let phases = test_phases(false);
    let ignored = phases.last().expect("ignored contract phase");

    assert!(ignored.args.contains(&"--lib"));
    assert!(ignored.args.contains(&"--ignored"));
    assert!(ignored.args.contains(&"--exact"));
}

#[test]
fn release_orchestrator_leaves_only_normal_library_tests_to_coverage() {
    let phases = test_phases(true);

    assert!(!phases.iter().any(|phase| phase.name == "Terlan library"));
    assert!(phases
        .iter()
        .any(|phase| phase.name == "workspace support crates"));
    assert!(phases
        .iter()
        .any(|phase| phase.name == "ignored std collection contract"));
}
