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
    let quality = phase_with_feature_filter("quality", "quality::");

    assert!(quality.args.contains(&"--lib"));
    assert!(quality.args.contains(&"quality::"));
    assert!(!quality.args.contains(&"--bin"));
}

#[test]
fn orchestrator_enables_every_feature_gated_harness() {
    let lsp = phase_with_feature_filter("LSP", "lsp::");

    assert!(lsp
        .args
        .windows(2)
        .any(|arguments| arguments == ["--features", "quality-tools,editor-lsp,benchmark-tools"]));
    for name in ["quality", "LSP", "benchmark harness"] {
        let phase = test_phases(false)
            .into_iter()
            .find(|phase| phase.name == name)
            .expect("feature-gated phase");
        assert!(phase.args.windows(2).any(
            |arguments| arguments == ["--features", "quality-tools,editor-lsp,benchmark-tools"]
        ));
    }
    assert!(!test_phases(false)
        .first()
        .expect("Terlan library phase")
        .args
        .contains(&"--features"));

    let integration = test_phases(false)
        .into_iter()
        .find(|phase| phase.name == "cross-feature integration")
        .expect("cross-feature integration phase");
    assert!(integration.args.contains(&"comprehension"));
    assert!(integration
        .args
        .windows(2)
        .any(|arguments| arguments == ["--features", "quality-tools,editor-lsp,benchmark-tools"]));
}

#[test]
fn orchestrator_runs_ignored_contract_once_in_the_library() {
    let phases = test_phases(false);
    let ignored: Vec<_> = phases
        .iter()
        .filter(|phase| phase.args.contains(&"--ignored"))
        .collect();

    assert_eq!(ignored.len(), 2);
    assert!(ignored.iter().all(|phase| phase.args.contains(&"--lib")));
    assert!(ignored.iter().all(|phase| phase.args.contains(&"--exact")));
    assert!(ignored
        .iter()
        .any(|phase| phase.name == "generated C++ package evidence"));
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
    assert!(phases
        .iter()
        .any(|phase| phase.name == "generated C++ package evidence"));
}
