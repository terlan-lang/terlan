//! Declarative test-tier selectors, separate from execution and admission.

use crate::{native_worker_path, PhaseExecutor, TestPhase, ValidationTier, INTEGRATION_FILTERS};

/// Declares each correctness tier once, preserving execution order.
pub(super) fn test_phases(coverage_owns_terlc: bool) -> Vec<TestPhase> {
    let mut phases = vec![
        terlan_integration_phase(),
        workspace_support_phase(),
        workspace_doctest_phase(),
        generated_cpp_package_phase(),
        ignored_std_collection_phase(),
        capability_worker_transport_phase(),
        capability_worker_sandbox_phase(),
        capability_event_pump_phase(),
        capability_protocol_reactor_phase(),
        epmd_bootstrap_phase(),
    ];
    if !coverage_owns_terlc {
        phases.insert(0, terlan_library_phase());
    }
    phases
}

/// Selects the normal library partition of the shared Terlan harness.
pub(super) fn terlan_library_phase() -> TestPhase {
    let mut args = Vec::new();
    for filter in INTEGRATION_FILTERS {
        args.extend(["--skip", filter]);
    }
    TestPhase {
        name: "Terlan library",
        tier: ValidationTier::FastUnit,
        executor: PhaseExecutor::TerlanHarness,
        args,
        environment: Vec::new(),
    }
}

/// Selects the integration partition without replaying normal library tests.
pub(super) fn terlan_integration_phase() -> TestPhase {
    let mut args = Vec::new();
    args.extend(INTEGRATION_FILTERS);
    TestPhase {
        name: "Terlan union-feature integration",
        tier: ValidationTier::Integration,
        executor: PhaseExecutor::TerlanHarness,
        args,
        environment: Vec::new(),
    }
}

fn workspace_support_phase() -> TestPhase {
    TestPhase {
        name: "workspace support crates",
        tier: ValidationTier::Integration,
        executor: PhaseExecutor::CargoNative,
        args: workspace_native_arguments(),
        environment: Vec::new(),
    }
}

/// Identical build and execution selection keeps the main library's Cargo unit reusable.
pub(super) fn workspace_native_arguments() -> Vec<&'static str> {
    vec![
        "test",
        "--locked",
        "--workspace",
        "--tests",
        "--features",
        "terlan/quality-tools,terlan/editor-lsp,terlan/benchmark-tools",
        "--message-format=json",
    ]
}

/// Keeps Rustdoc execution independent of the native Cargo runner.
pub(super) fn workspace_doctest_phase() -> TestPhase {
    TestPhase {
        name: "workspace support doctests",
        tier: ValidationTier::Integration,
        executor: PhaseExecutor::Cargo,
        args: vec![
            "test",
            "--locked",
            "--workspace",
            "--exclude",
            "terlan",
            "--doc",
            "--",
        ],
        environment: Vec::new(),
    }
}

fn generated_cpp_package_phase() -> TestPhase {
    TestPhase {
        name: "generated C++ package evidence",
        tier: ValidationTier::AotNativeLink,
        executor: PhaseExecutor::TerlanHarness,
        args: vec![
            "commands::bind::cpp_package_consumer_test::generated_cpp_git_package_executes_and_rejects_stale_handles",
            "--ignored",
            "--exact",
        ],
        environment: Vec::new(),
    }
}

/// Owns the exact ignored standard-library collection contract.
pub(super) fn ignored_std_collection_phase() -> TestPhase {
    TestPhase {
        name: "ignored std collection contract",
        tier: ValidationTier::Integration,
        executor: PhaseExecutor::TerlanHarness,
        args: vec![
            "compiler::typeck::std_contract_test::syntax_output_accepts_release_core_collection_contracts",
            "--ignored",
            "--exact",
        ],
        environment: Vec::new(),
    }
}

fn ignored_native_phase(
    name: &'static str,
    selector: &'static str,
    environment: Vec<(&'static str, String)>,
) -> TestPhase {
    TestPhase {
        name,
        tier: ValidationTier::AotNativeLink,
        executor: PhaseExecutor::TerlanHarness,
        args: vec![selector, "--ignored", "--exact"],
        environment,
    }
}

fn capability_worker_transport_phase() -> TestPhase {
    ignored_native_phase(
        "capability worker process transport",
        "runtime::vm::capability_worker::capability_worker_test::capability_worker_process_transport_runs_full_cycle",
        vec![("TERLAN_TEST_CAPABILITY_WORKER", native_worker_path())],
    )
}

fn capability_worker_sandbox_phase() -> TestPhase {
    ignored_native_phase(
        "capability worker sandbox descriptor closure",
        "runtime::vm::capability_worker::capability_worker_test::capability_worker_sandbox_closes_inherited_descriptor",
        vec![("TERLAN_TEST_CAPABILITY_WORKER", native_worker_path())],
    )
}

fn capability_event_pump_phase() -> TestPhase {
    ignored_native_phase(
        "generated capability event pump",
        "commands::serve::handler_cache::invocation::invocation_test::generated_capability_event_pump_executes_real_worker_full_cycle",
        vec![
            ("TERLAN_NATIVE_WORKER", native_worker_path()),
            ("TERLAN_TEST_AOT_CAPABILITY_PUMP", "1".to_string()),
            ("TERLAN_TEST_CAPABILITY_NETWORK_SANDBOX", "1".to_string()),
        ],
    )
}

fn capability_protocol_reactor_phase() -> TestPhase {
    ignored_native_phase(
        "capability protocol reactor wakeup",
        "commands::serve::handler_cache::invocation::invocation_protocol_test::protocol_reactor_capability_worker_wakes_and_resumes_exact_actor",
        vec![("TERLAN_NATIVE_WORKER", native_worker_path())],
    )
}

fn epmd_bootstrap_phase() -> TestPhase {
    TestPhase {
        name: "EPMD discovery transport full cycle",
        tier: ValidationTier::ControlledHost,
        executor: PhaseExecutor::TerlanHarness,
        args: vec![
            "runtime::vm::epmd::epmd_test::logical_node_bootstrap_runs_discovery_transport_and_shutdown_full_cycle",
            "--ignored",
            "--exact",
        ],
        environment: Vec::new(),
    }
}
