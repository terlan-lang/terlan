use super::*;

/// Verifies the default VM instrumentation provider is local-only.
///
/// Inputs:
/// - Default local VM instrumentation provider.
///
/// Output:
/// - Stable id, local scope, local-process transport, and successful
///   validation.
///
/// Transformation:
/// - Locks the first TUI provider to the standalone VM process instead of
///   Terlan Cloud.
#[test]
fn default_vm_instrumentation_provider_is_local_only() {
    let provider = default_local_vm_instrumentation_provider();

    assert_eq!(provider.id, "local.vm");
    assert_eq!(provider.display_name, "Local Terlan VM");
    assert_eq!(provider.scope.as_str(), "local_vm");
    assert_eq!(provider.transport.as_str(), "local_process");
    assert_eq!(validate_vm_instrumentation_providers(&[provider]), Ok(()));
}

/// Verifies VM instrumentation cannot be configured without a local provider.
///
/// Inputs:
/// - Empty provider list.
///
/// Output:
/// - Stable missing-provider diagnostic.
///
/// Transformation:
/// - Prevents local VM instrumentation from depending on an implicit cloud
///   provider or remote dashboard.
#[test]
fn vm_instrumentation_rejects_missing_local_provider() {
    let diagnostics = validate_vm_instrumentation_providers(&[]).expect_err("missing provider");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "vm_instrumentation_missing_provider");
}

/// Verifies local provider ids must be explicit.
///
/// Inputs:
/// - Provider with blank id but otherwise local scope and transport.
///
/// Output:
/// - Stable empty-provider-id diagnostic.
///
/// Transformation:
/// - Keeps TUI provider records addressable without requiring cloud identity.
#[test]
fn vm_instrumentation_rejects_empty_provider_id() {
    let mut provider = default_local_vm_instrumentation_provider();
    provider.id = "";

    let diagnostics =
        validate_vm_instrumentation_providers(&[provider]).expect_err("empty provider id");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "vm_instrumentation_empty_provider_id");
}

/// Verifies local VM instrumentation rejects cloud provider descriptors.
///
/// Inputs:
/// - Cloud VM provider descriptor.
///
/// Output:
/// - Stable non-local diagnostics.
///
/// Transformation:
/// - Keeps the standalone VM TUI independent from cloud scope and cloud
///   transport even though shared dashboard components know both providers.
#[test]
fn vm_instrumentation_rejects_cloud_provider_for_local_tui() {
    let diagnostics = validate_vm_instrumentation_providers(&[cloud_vm_instrumentation_provider()])
        .expect_err("cloud provider is not local");
    let codes = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect::<Vec<_>>();

    assert!(codes.contains(&"vm_instrumentation_non_local_scope"));
    assert!(codes.contains(&"vm_instrumentation_non_local_transport"));
}

/// Verifies local and cloud providers share dashboard components.
///
/// Inputs:
/// - Local and cloud provider descriptors.
///
/// Output:
/// - Identical component ids and kind spellings.
///
/// Transformation:
/// - Keeps provider transport separate from the logical dashboard surface that
///   Ratatui and cloud UIs will render.
#[test]
fn vm_dashboard_components_are_shared_by_local_and_cloud_providers() {
    let local = default_local_vm_instrumentation_provider();
    let cloud = cloud_vm_instrumentation_provider();
    let local_components = vm_dashboard_components_for_provider(&local).expect("local components");
    let cloud_components = vm_dashboard_components_for_provider(&cloud).expect("cloud components");

    assert_eq!(local_components, cloud_components);
    assert_eq!(
        local_components
            .iter()
            .map(|component| component.id)
            .collect::<Vec<_>>(),
        vec!["runtime", "processes", "queues", "native-boundary"]
    );
    assert_eq!(
        local_components
            .iter()
            .map(|component| component.kind.as_str())
            .collect::<Vec<_>>(),
        vec![
            "runtime_overview",
            "process_list",
            "message_queues",
            "native_boundary"
        ]
    );
}

/// Verifies shared dashboard components still require provider identity.
///
/// Inputs:
/// - Provider descriptor with blank id.
///
/// Output:
/// - Stable empty-provider-id diagnostic.
///
/// Transformation:
/// - Prevents local/cloud component rendering from losing source provider
///   identity.
#[test]
fn vm_dashboard_components_reject_empty_provider_id() {
    let mut provider = cloud_vm_instrumentation_provider();
    provider.id = "";

    let diagnostics =
        vm_dashboard_components_for_provider(&provider).expect_err("empty provider id");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "vm_instrumentation_empty_provider_id");
}

/// Verifies the default local VM dashboard is read-only.
///
/// Inputs:
/// - Default local VM dashboard config.
///
/// Output:
/// - Read-only mode and successful validation.
///
/// Transformation:
/// - Locks the first dashboard version to inspection-only behavior.
#[test]
fn default_local_vm_dashboard_is_read_only() {
    let config = default_local_vm_dashboard_config();

    assert_eq!(config.mode.as_str(), "read_only");
    assert_eq!(config.provider.id, "local.vm");
    assert!(!config.components.is_empty());
    assert_eq!(validate_vm_dashboard_config(&config), Ok(()));
}

/// Verifies v1 dashboard validation rejects operator mode.
///
/// Inputs:
/// - Local dashboard config switched to operator mode.
///
/// Output:
/// - Stable operator-mode-disabled diagnostic.
///
/// Transformation:
/// - Prevents terminal UI controls from mutating VM state before guarded
///   operator policy exists.
#[test]
fn vm_dashboard_v1_rejects_operator_mode() {
    let mut config = default_local_vm_dashboard_config();
    config.mode = VmDashboardMode::Operator;

    let diagnostics = validate_vm_dashboard_config(&config).expect_err("operator disabled");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "vm_dashboard_operator_mode_disabled");
}

/// Verifies read-only dashboards still require components.
///
/// Inputs:
/// - Local read-only dashboard config with no components.
///
/// Output:
/// - Stable missing-components diagnostic.
///
/// Transformation:
/// - Prevents a formally read-only dashboard from rendering no inspection
///   surface.
#[test]
fn vm_dashboard_rejects_empty_component_set() {
    let mut config = default_local_vm_dashboard_config();
    config.components.clear();

    let diagnostics = validate_vm_dashboard_config(&config).expect_err("missing components");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "vm_dashboard_missing_components");
}

/// Verifies the planned operator action vocabulary is stable.
///
/// Inputs:
/// - Planned VM operator actions.
///
/// Output:
/// - Stable action spellings for future guarded mode.
///
/// Transformation:
/// - Records mutating operator intent without enabling the actions in v1.
#[test]
fn vm_operator_actions_have_stable_names() {
    let action_names = planned_vm_operator_actions()
        .iter()
        .map(|action| action.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        action_names,
        vec![
            "hot_reload",
            "deploy",
            "rollback",
            "node_drain",
            "service_restart",
            "replica_promotion"
        ]
    );
}

/// Verifies the default operator policy is disabled and auditable.
///
/// Inputs:
/// - Default operator policy.
///
/// Output:
/// - Successful validation with no enabled actions.
///
/// Transformation:
/// - Keeps the v1 dashboard read-only while preserving the future guard shape.
#[test]
fn default_vm_operator_policy_is_disabled() {
    let policy = default_vm_operator_policy();

    assert!(!policy.enabled);
    assert!(policy.audit_required);
    assert!(policy.actions.is_empty());
    assert_eq!(validate_vm_operator_policy(&policy), Ok(()));
}

/// Verifies enabled operator policies are rejected in v1.
///
/// Inputs:
/// - Policy with one planned mutating action enabled.
///
/// Output:
/// - Stable operator-mode-not-available diagnostic.
///
/// Transformation:
/// - Prevents hot reload, deploy, rollback, or similar controls from becoming
///   active before guarded operator semantics exist.
#[test]
fn vm_operator_policy_rejects_enabled_actions_in_v1() {
    let mut policy = default_vm_operator_policy();
    policy.enabled = true;
    policy.actions = vec![VmOperatorAction::HotReload];

    let diagnostics = validate_vm_operator_policy(&policy).expect_err("operator disabled");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "vm_operator_mode_not_available");
}

/// Verifies operator policies always require audit semantics.
///
/// Inputs:
/// - Disabled policy with audit disabled.
///
/// Output:
/// - Stable audit-required diagnostic.
///
/// Transformation:
/// - Ensures future operator mode cannot be enabled without an audit contract.
#[test]
fn vm_operator_policy_requires_audit() {
    let mut policy = default_vm_operator_policy();
    policy.audit_required = false;

    let diagnostics = validate_vm_operator_policy(&policy).expect_err("audit required");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "vm_operator_audit_required");
}

/// Verifies VM runtime inspection exposes OTP-grade resiliency surfaces without
/// exposing OTP structure.
///
/// Inputs:
/// - Required VM runtime inspection surface names.
///
/// Output:
/// - Stable Terlan-owned surface names for process, supervisor, resource,
///   native-call, timer, restart, cancellation, and source identity inspection.
///
/// Transformation:
/// - Locks inspection to Terlan VM concepts instead of VM module, opcode,
///   ERTS, or OTP application compatibility.
#[test]
fn vm_runtime_inspection_surfaces_are_terlan_owned() {
    let surfaces = required_vm_runtime_inspection_surfaces();

    assert_eq!(
        surfaces,
        vec![
            "process_registry",
            "process_tree",
            "supervisor_tree",
            "mailboxes",
            "process_state",
            "reductions",
            "heap_pressure",
            "timers",
            "timer_outcomes",
            "restart_history",
            "native_calls",
            "resource_handles",
            "cancellation",
            "source_identity",
        ]
    );
    assert!(surfaces
        .iter()
        .all(|surface| !surface.contains("otp") && !surface.contains("beam")));
}

/// Verifies local VM runtime inspection snapshots carry process, supervisor,
/// resource, timer, and source identity data.
///
/// Inputs:
/// - Local VM provider.
/// - Representative process, supervisor, resource, and timer rows.
///
/// Output:
/// - Provider-bound read-only snapshot preserving all supplied rows.
///
/// Transformation:
/// - Gives `terlan-vm inspect ...`, the local TUI, and future cloud dashboards
///   one typed data shape for runtime inspection.
#[test]
fn vm_runtime_inspection_snapshot_preserves_runtime_graph() {
    let provider = default_local_vm_instrumentation_provider();
    let process = VmProcessInspectionSnapshot {
        pid: "p-1".to_string(),
        parent_pid: None,
        supervisor_id: Some("sup-main".to_string()),
        source_module: "app.Counter".to_string(),
        source_function: "loop".to_string(),
        state: VmInspectedProcessState::Blocked,
        mailbox_len: 2,
        reductions: 128,
        heap_bytes: 4096,
        restart_count: 1,
        resource_handles: vec!["res-db-1".to_string()],
        native_call_state: Some("waiting:postgres.query".to_string()),
        cancellation_requested: false,
    };
    let supervisor = VmSupervisorInspectionSnapshot {
        id: "sup-main".to_string(),
        strategy: "one_for_one".to_string(),
        child_pids: vec!["p-1".to_string()],
        restart_count: 1,
    };
    let resource = VmResourceInspectionSnapshot {
        handle: "res-db-1".to_string(),
        owner_pid: "p-1".to_string(),
        kind: "postgres_connection".to_string(),
        state: "checked_out".to_string(),
    };
    let timer = VmTimerInspectionSnapshot {
        id: "timer-1".to_string(),
        owner_pid: "p-1".to_string(),
        remaining_ms: 250,
    };
    let timer_outcome = VmTimerOutcomeInspectionSnapshot {
        id: "timer-0".to_string(),
        owner_pid: "p-1".to_string(),
        kind: "one_shot".to_string(),
        outcome: "cancelled".to_string(),
        detail: None,
    };

    let snapshot = vm_runtime_inspection_snapshot(
        &provider,
        vec![process],
        vec![supervisor],
        vec![resource],
        vec![timer],
        vec![timer_outcome],
    )
    .expect("runtime inspection snapshot");

    assert_eq!(snapshot.provider_id, "local.vm");
    assert_eq!(snapshot.processes[0].state.as_str(), "blocked");
    assert_eq!(snapshot.processes[0].mailbox_len, 2);
    assert_eq!(snapshot.processes[0].source_module, "app.Counter");
    assert_eq!(snapshot.supervisors[0].child_pids, vec!["p-1"]);
    assert_eq!(snapshot.resources[0].kind, "postgres_connection");
    assert_eq!(snapshot.timers[0].remaining_ms, 250);
    assert_eq!(snapshot.timer_outcomes[0].outcome, "cancelled");
}
