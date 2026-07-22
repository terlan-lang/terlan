use super::{phase_with_feature_filter, phase_with_filter, test_phases};

#[test]
fn orchestrator_assigns_shared_runtime_tests_to_terlc_only() {
    let phases = test_phases(false);
    let terlc = phases.first().expect("terlc phase");
    let vm = phases
        .iter()
        .find(|phase| phase.name == "terlan-vm owned tests")
        .expect("VM phase");

    assert_eq!(terlc.args[5], "terlc");
    assert!(vm
        .args
        .windows(2)
        .any(|args| args == ["--skip", "runtime::"]));
    assert!(vm
        .args
        .windows(2)
        .any(|args| args == ["--skip", "compiler::"]));
}

#[test]
fn orchestrator_uses_owned_namespace_filters_for_embedded_harnesses() {
    let quality = phase_with_filter("quality", "terlan-quality", "terlan_quality::");

    assert_eq!(quality.args[5], "terlan-quality");
    assert_eq!(quality.args[6], "terlan_quality::");
    assert_eq!(quality.args[7], "--");
}

#[test]
fn orchestrator_enables_editor_feature_only_for_lsp() {
    let lsp = phase_with_feature_filter("LSP", "terlan-lsp", "editor-lsp", "terlan_lsp::");

    assert_eq!(&lsp.args[4..6], ["--features", "editor-lsp"]);
    assert!(!test_phases(false)
        .first()
        .expect("terlc phase")
        .args
        .contains(&"--features"));
}

#[test]
fn orchestrator_runs_ignored_contract_once_in_terlc() {
    let phases = test_phases(false);
    let ignored = phases.last().expect("ignored contract phase");

    assert_eq!(ignored.args[5], "terlc");
    assert!(ignored.args.contains(&"--ignored"));
    assert!(ignored.args.contains(&"--exact"));
}

#[test]
fn release_orchestrator_leaves_only_normal_terlc_tests_to_coverage() {
    let phases = test_phases(true);

    assert!(!phases.iter().any(|phase| phase.name == "terlc"));
    assert!(phases
        .iter()
        .any(|phase| phase.name == "terlan-vm owned tests"));
    assert!(phases
        .iter()
        .any(|phase| phase.name == "ignored std collection contract"));
}
