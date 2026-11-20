//! Declarative test-tier selectors, separate from execution and admission.

use crate::{
    native_worker_path, PhaseExecutor, TestPhase, TestShard, ValidationTier, INTEGRATION_FILTERS,
};
use std::ffi::OsStr;

pub(super) const COMPACT_PHASE_PLAN_ENV: &str = "TERLAN_TEST_COMPACT_PHASE_PLAN";

/// Selects the recorded production or compact acceptance plan from one strict setting.
pub(super) fn selected_test_phases(
    coverage_owns_terlc: bool,
    setting: Option<&OsStr>,
) -> Result<Vec<TestPhase>, crate::PhaseFailure> {
    match setting {
        None => Ok(test_phases(coverage_owns_terlc)),
        Some(value) if value.is_empty() || value == OsStr::new("0") => {
            Ok(test_phases(coverage_owns_terlc))
        }
        Some(value) if value == OsStr::new("1") => Ok(compact_test_phases(coverage_owns_terlc)),
        Some(_) => Err(crate::PhaseFailure {
            outcome: "suite-inputs-failed",
            detail: "invalid compact test phase plan setting".into(),
        }),
    }
}

/// Declares each correctness tier once, preserving execution order.
pub(super) fn test_phases(coverage_owns_terlc: bool) -> Vec<TestPhase> {
    let mut phases = candidate_contract_phases();
    if !coverage_owns_terlc {
        phases.extend(terlan_library_partitions());
    }
    phases.push(workspace_support_phase());
    phases.push(workspace_integration_phase());
    phases.push(workspace_doctest_phase());
    phases.extend(platform_evidence_phases());
    phases
}

/// Keeps synthetic acceptance workspaces small without weakening production phases.
pub(super) fn compact_test_phases(coverage_owns_terlc: bool) -> Vec<TestPhase> {
    let mut phases = candidate_contract_phases();
    if !coverage_owns_terlc {
        phases.push(terlan_library_phase());
    }
    phases.push(workspace_support_phase());
    phases.push(workspace_integration_phase());
    phases.push(workspace_doctest_phase());
    phases.extend(platform_evidence_phases());
    phases
}

/// Cheap deterministic contracts run immediately after the shared build.
/// Keeping them in one list makes it impossible to add a candidate contract
/// after broad compiler suites without changing this explicit boundary.
fn candidate_contract_phases() -> Vec<TestPhase> {
    vec![terlan_integration_phase(), ignored_std_collection_phase()]
}

/// Host, linker, and process-bound evidence remains after source correctness.
fn platform_evidence_phases() -> Vec<TestPhase> {
    let mut phases = vec![
        generated_cpp_package_phase(),
        capability_worker_transport_phase(),
        capability_worker_sandbox_phase(),
        capability_event_pump_phase(),
        capability_protocol_reactor_phase(),
        epmd_bootstrap_phase(),
    ];
    // Keep the branch in every target's lint/type-check graph. These phase
    // constructors are target-neutral; only their selection is Linux-only.
    // Attribute-removing this branch makes `phases` appear immutable on
    // macOS and lets host-specific warning drift reach the release matrix.
    if cfg!(target_os = "linux") {
        phases.push(capability_storage_phase());
        phases.push(public_storage_phase());
    }
    phases
}

/// Splits the shared harness into bounded, independently observed serial owners.
/// Keep each process serial: compiler fixtures change process-wide environment.
/// Libtest filters are substrings, so each owner excludes every earlier selector.
pub(super) fn terlan_library_partitions() -> Vec<TestPhase> {
    let groups: &[(&str, &[&str])] = &[
        (
            "Terlan build arguments",
            &[
                "commands::build::build_test::tests::annotation_isolation_artifact_test::",
                "commands::build::build_test::tests::args_test::",
            ],
        ),
        (
            "Terlan browser build artifacts",
            &["commands::build::build_test::tests::artifact_test::browser_artifacts::"],
        ),
        (
            "Terlan VM build artifacts",
            &["commands::build::build_test::tests::artifact_test::vm_artifacts::"],
        ),
        (
            "Terlan build debug evidence",
            &[
                "commands::build::build_test::tests::asm_labels_artifact_test::",
                "commands::build::build_test::tests::debug_info_artifact_test::",
                "commands::build::build_test::tests::deterministic_artifact_test::",
                "commands::build::build_test::tests::embedded_line_coverage_artifact_test::",
                "commands::build::build_test::tests::key_compatibility_test::",
                "commands::build::build_test::tests::latin1_source_policy_test::",
            ],
        ),
        (
            "Terlan build dependency closure",
            &[
                "commands::build::build_test::tests::dependency_test::",
                "commands::build::build_test::tests::executable_vm_artifact_test::",
                "commands::build::build_test::tests::import_constructor_test::",
            ],
        ),
        (
            "Terlan build project layout",
            &[
                "commands::build::build_test::tests::parallel_compilation_test::",
                "commands::build::build_test::tests::project_layout_test::",
            ],
        ),
        (
            "Terlan JavaScript build shapes",
            &[
                "commands::build::build_test::tests::js_target_diagnostics_test::",
                "commands::build::build_test::tests::shape_js_test::",
            ],
        ),
        ("Terlan build test remainder", &["commands::build::build_test::"]),
        (
            "Terlan build manifests",
            &["commands::build::project_manifest::"],
        ),
        (
            "Terlan VM artifact builds",
            &["commands::build::vm_artifact::"],
        ),
        (
            "Terlan build orchestration",
            &["commands::build::build_orchestration::"],
        ),
        (
            "Terlan browser build primitives",
            &[
                "commands::build::css::",
                "commands::build::js::",
                "commands::build::js_source_classification::",
            ],
        ),
        (
            "Terlan browser manifest discovery",
            &["commands::build::js_browser::js_browser_test::asset_and_response_manifests::discover_web_handlers"],
        ),
        (
            "Terlan browser manifest validation",
            &["commands::build::js_browser::js_browser_test::asset_and_response_manifests::write_browser_manifest"],
        ),
        (
            "Terlan browser package source",
            &["commands::build::js_browser::js_browser_test::asset_and_response_manifests::write_browser_package"],
        ),
        (
            "Terlan browser callbacks",
            &[
                "commands::build::js_browser::js_browser_test::callback_source_execution::",
                "commands::build::js_browser::js_browser_test::response_source_execution::",
            ],
        ),
        (
            "Terlan browser handler routes",
            &["commands::build::js_browser::js_browser_test::route_fixtures::discover_web_handlers"],
        ),
        (
            "Terlan browser route manifests",
            &["commands::build::js_browser::js_browser_test::route_fixtures::discover_web_route_manifest"],
        ),
        (
            "Terlan browser route serialization",
            &["commands::build::js_browser::js_browser_test::route_fixtures::write_browser_package"],
        ),
        (
            "Terlan browser VM routes",
            &[
                "commands::build::js_browser::js_browser_test::vm_service_staging",
                "commands::build::js_browser::js_browser_test::vm_source_routes::",
            ],
        ),
        (
            "Terlan browser WebSockets",
            &["commands::build::js_browser::js_browser_test::websocket_source_execution::"],
        ),
        (
            "Terlan package registry builds",
            &["commands::build::package_registry"],
        ),
        (
            "Terlan package publication builds",
            &[
                "commands::build::package_artifact::",
                "commands::build::package_git::",
                "commands::build::package_publish",
            ],
        ),
        ("Terlan build command remainder", &["commands::build::"]),
        (
            "Terlan serve arguments",
            &["commands::serve::serve_test::arguments_and_fixtures::"],
        ),
        (
            "Terlan dynamic serve dispatch",
            &["commands::serve::serve_test::dynamic_dispatch::"],
        ),
        (
            "Terlan serve package validation",
            &["commands::serve::serve_test::observability_and_packages::"],
        ),
        (
            "Terlan serve route dispatch",
            &["commands::serve::serve_test::route_dispatch::"],
        ),
        (
            "Terlan serve static fallbacks",
            &["commands::serve::serve_test::static_fallbacks::"],
        ),
        (
            "Terlan serve upgrades",
            &["commands::serve::serve_test::upgrades_and_acme::"],
        ),
        (
            "Terlan serve cache invocation",
            &["commands::serve::handler_cache::invocation::"],
        ),
        (
            "Terlan serve response policy",
            &["commands::serve::handler_cache::response_policy_test::"],
        ),
        (
            "Terlan serve request routing",
            &[
                "commands::serve::handler_cache::request_projection::",
                "commands::serve::handler_cache::source_router::",
            ],
        ),
        (
            "Terlan serve cache generation",
            &[
                "commands::serve::handler_cache::handler_cache_generation_test::",
                "commands::serve::handler_cache::multicore_performance_test::",
                "commands::serve::handler_cache::shard_owner::",
            ],
        ),
        (
            "Terlan serve cache protocols",
            &[
                "commands::serve::handler_cache::closure_invocation_test::",
                "commands::serve::handler_cache::protocol_capability::",
            ],
        ),
        (
            "Terlan serve response bridge",
            &["commands::serve::handler::response_bridge::"],
        ),
        (
            "Terlan serve WebSocket invocation",
            &["commands::serve::handler::websocket_invocation::"],
        ),
        ("Terlan serve handler remainder", &["commands::serve::handler::"]),
        (
            "Terlan TLS cache custody",
            &["commands::serve::tls::acme_runtime::tls_test::cache_custody::"],
        ),
        (
            "Terlan TLS and ACME fixtures",
            &["commands::serve::tls::acme_runtime::tls_test::tls_and_acme_fixtures::"],
        ),
        (
            "Terlan Hyper JSON bodies",
            &["commands::serve::hyper_server::hyper_server_test::json_body::"],
        ),
        (
            "Terlan Hyper SSE source",
            &["commands::serve::hyper_server::hyper_server_test::sse_source::"],
        ),
        ("Terlan Hyper remainder", &["commands::serve::hyper_server::"]),
        ("Terlan serve command remainder", &["commands::serve::"]),
        (
            "Terlan C bindings",
            &["commands::bind::c_abi_binding_generator::"],
        ),
        (
            "Terlan C++ bindings",
            &["commands::bind::cpp_binding_generator::"],
        ),
        ("Terlan binding command remainder", &["commands::bind::"]),
        ("Terlan lint commands", &["commands::lint::"]),
        ("Terlan JavaScript emission", &["commands::emit_js::"]),
        ("Terlan database commands", &["commands::db::"]),
        ("Terlan test commands", &["commands::test::"]),
        ("Terlan command remainder", &["commands::"]),
        ("Terlan native IR", &["compiler::native_ir::"]),
        ("Terlan type checker", &["compiler::typeck::"]),
        ("Terlan compiler", &["compiler::"]),
        ("Terlan runtime", &["runtime::"]),
        ("Terlan library remainder", &[]),
    ];
    let mut previous_filters = Vec::new();
    groups
        .iter()
        .copied()
        .map(|(name, filters)| {
            let mut phase = terlan_library_phase();
            phase.name = name;
            phase.test_threads = match name {
                "Terlan serve cache invocation"
                | "Terlan serve response policy"
                | "Terlan serve request routing"
                | "Terlan Hyper JSON bodies"
                | "Terlan Hyper SSE source" => Some(4),
                _ => None,
            };
            phase.args.extend_from_slice(filters);
            for &filter in &previous_filters {
                phase.args.extend(["--skip", filter]);
            }
            previous_filters.extend_from_slice(filters);
            phase
        })
        .flat_map(|phase| {
            let total = match phase.name {
                "Terlan C bindings"
                | "Terlan C++ bindings"
                | "Terlan lint commands"
                | "Terlan JavaScript emission" => 4,
                "Terlan database commands" | "Terlan test commands" => 3,
                "Terlan command remainder"
                | "Terlan native IR"
                | "Terlan type checker"
                | "Terlan compiler"
                | "Terlan runtime"
                | "Terlan library remainder" => 32,
                _ => 1,
            };
            shard_phase(phase, total)
        })
        .collect()
}

fn shard_phase(phase: TestPhase, total: usize) -> Vec<TestPhase> {
    if total == 1 {
        return vec![phase];
    }
    (0..total)
        .map(|index| {
            let mut shard = phase.clone();
            shard.name = Box::leak(
                format!("{} shard {:02}/{total:02}", phase.name, index + 1).into_boxed_str(),
            );
            shard.shard = Some(TestShard { index, total });
            shard
        })
        .collect()
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
        shard: None,
        test_threads: None,
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
        shard: None,
        test_threads: None,
    }
}

fn workspace_support_phase() -> TestPhase {
    TestPhase {
        name: "workspace support unit targets",
        tier: ValidationTier::Integration,
        executor: PhaseExecutor::CargoNative,
        args: workspace_support_arguments(),
        environment: Vec::new(),
        shard: None,
        test_threads: None,
    }
}

fn workspace_integration_phase() -> TestPhase {
    TestPhase {
        name: "workspace integration targets",
        tier: ValidationTier::Integration,
        executor: PhaseExecutor::CargoNative,
        args: workspace_integration_arguments(),
        environment: Vec::new(),
        shard: None,
        test_threads: None,
    }
}

const UNION_FEATURES: &str = "terlan/validation-union";

pub(super) fn cargo_test_arguments(selection: &[&'static str]) -> Vec<&'static str> {
    let mut arguments = vec!["test", "--profile", "dev", "--locked"];
    arguments.extend_from_slice(selection);
    arguments.push("--message-format=json");
    arguments
}

pub(super) fn union_feature_arguments(selection: &[&'static str]) -> Vec<&'static str> {
    let mut arguments = cargo_test_arguments(selection);
    arguments.splice(
        arguments.len() - 1..arguments.len() - 1,
        ["--features", UNION_FEATURES],
    );
    arguments
}

/// Builds only the directly executed library harness before its early phases.
pub(super) fn terlan_harness_arguments() -> Vec<&'static str> {
    union_feature_arguments(&["-p", "terlan", "--lib"])
}

/// Runs non-Terlan unit targets without selecting the already-owned compiler targets.
pub(super) fn workspace_support_arguments() -> Vec<&'static str> {
    cargo_test_arguments(&["--workspace", "--exclude", "terlan", "--lib", "--bins"])
}

/// Runs every integration target without selecting any package library or binary target.
pub(super) fn workspace_integration_arguments() -> Vec<&'static str> {
    cargo_test_arguments(&["--workspace", "--test", "*"])
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
        shard: None,
        test_threads: None,
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
        shard: None,
        test_threads: None,
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
        shard: None,
        test_threads: None,
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
        shard: None,
        test_threads: None,
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

/// Runs durable storage through the real sandboxed worker, never a model executor.
fn capability_storage_phase() -> TestPhase {
    ignored_native_phase(
        "capability worker durable storage",
        "runtime::vm::capability_worker::capability_worker_test::storage_process_test::capability_storage_worker_survives_restart_with_durable_checkpoint",
        vec![("TERLAN_TEST_CAPABILITY_WORKER", native_worker_path())],
    )
}

/// Proves the public source path across complete VM shutdown, using the shared build.
fn public_storage_phase() -> TestPhase {
    ignored_native_phase(
        "public durable storage AOT restart",
        "vm::main_test::storage_lifecycle_test::public_storage_aot_survives_a_complete_vm_restart",
        vec![
            ("TERLAN_TEST_COMPILER", crate::prebuilt_binary_path("terlc")),
            ("TERLAN_TEST_VM", crate::prebuilt_binary_path("terlan-vm")),
            ("TERLAN_TEST_CAPABILITY_WORKER", native_worker_path()),
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
        shard: None,
        test_threads: None,
    }
}
