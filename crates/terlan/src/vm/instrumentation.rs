//! Local Terlan VM instrumentation provider model.
//!
//! Inputs:
//! - Dashboard provider declarations for the standalone `terlan-vm` binary.
//!
//! Outputs:
//! - Validated local VM provider configuration.
//!
//! Transformation:
//! - Keeps the local VM instrumentation surface independent from Terlan Cloud
//!   by admitting only local-process providers.

/// Provider scope accepted by the local VM instrumentation surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmInstrumentationProviderScope {
    /// Provider reads state from the current local VM process.
    LocalVm,
    /// Provider reads state from a Terlan Cloud operator API.
    TerlanCloud,
}

impl VmInstrumentationProviderScope {
    /// Returns the stable provider scope spelling.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::LocalVm => "local_vm",
            Self::TerlanCloud => "terlan_cloud",
        }
    }
}

/// Provider transport accepted by the local VM instrumentation surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmInstrumentationTransport {
    /// Provider reads in-process VM state without network or cloud APIs.
    LocalProcess,
    /// Provider reads remote operator state through a cloud API.
    CloudApi,
}

impl VmInstrumentationTransport {
    /// Returns the stable transport spelling.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::LocalProcess => "local_process",
            Self::CloudApi => "cloud_api",
        }
    }
}

/// One VM instrumentation provider.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmInstrumentationProvider {
    pub(crate) id: &'static str,
    pub(crate) display_name: &'static str,
    pub(crate) scope: VmInstrumentationProviderScope,
    pub(crate) transport: VmInstrumentationTransport,
}

/// Provider-neutral dashboard component kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmDashboardComponentKind {
    /// High-level runtime identity and status.
    RuntimeOverview,
    /// Process table or equivalent actor list.
    ProcessList,
    /// Message queue and mailbox pressure summary.
    MessageQueues,
    /// Native-boundary request and resource summary.
    NativeBoundary,
}

impl VmDashboardComponentKind {
    /// Returns the stable component kind spelling.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::RuntimeOverview => "runtime_overview",
            Self::ProcessList => "process_list",
            Self::MessageQueues => "message_queues",
            Self::NativeBoundary => "native_boundary",
        }
    }
}

/// One provider-neutral dashboard component.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmDashboardComponent {
    pub(crate) id: &'static str,
    pub(crate) title: &'static str,
    pub(crate) kind: VmDashboardComponentKind,
}

/// Dashboard mode accepted by the VM instrumentation UI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmDashboardMode {
    /// Read-only inspection mode.
    ReadOnly,
    /// Future guarded operator mode.
    Operator,
}

impl VmDashboardMode {
    /// Returns the stable dashboard mode spelling.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::Operator => "operator",
        }
    }
}

/// Provider-bound dashboard configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmDashboardConfig {
    pub(crate) mode: VmDashboardMode,
    pub(crate) provider: VmInstrumentationProvider,
    pub(crate) components: Vec<VmDashboardComponent>,
}

/// Future guarded VM operator action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmOperatorAction {
    /// Hot reload VM code or loaded artifacts.
    HotReload,
    /// Deploy a runtime artifact.
    Deploy,
    /// Roll back to a previous runtime artifact.
    Rollback,
    /// Drain a runtime node.
    NodeDrain,
    /// Restart a runtime service.
    ServiceRestart,
    /// Promote a replica.
    ReplicaPromotion,
}

impl VmOperatorAction {
    /// Returns the stable operator action spelling.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::HotReload => "hot_reload",
            Self::Deploy => "deploy",
            Self::Rollback => "rollback",
            Self::NodeDrain => "node_drain",
            Self::ServiceRestart => "service_restart",
            Self::ReplicaPromotion => "replica_promotion",
        }
    }
}

/// Guard policy for future VM operator mode.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmOperatorPolicy {
    pub(crate) enabled: bool,
    pub(crate) audit_required: bool,
    pub(crate) actions: Vec<VmOperatorAction>,
}

/// Validation diagnostic for VM instrumentation provider configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmInstrumentationDiagnostic {
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

/// Text snapshot produced by the local VM dashboard renderer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmDashboardRenderSnapshot {
    pub(crate) provider_id: String,
    pub(crate) mode: &'static str,
    pub(crate) component_ids: Vec<String>,
    pub(crate) text: String,
}

/// Runtime process state exposed by Terlan VM inspection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmInspectedProcessState {
    /// Process is ready to execute VM reductions.
    Runnable,
    /// Process is waiting on a message, timer, resource, or native call.
    Blocked,
    /// Process has exited and remains visible through inspection history.
    Exited,
}

impl VmInspectedProcessState {
    /// Returns the stable process-state spelling.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Runnable => "runnable",
            Self::Blocked => "blocked",
            Self::Exited => "exited",
        }
    }
}

/// Read-only process snapshot exposed by VM inspection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmProcessInspectionSnapshot {
    pub(crate) pid: String,
    pub(crate) parent_pid: Option<String>,
    pub(crate) supervisor_id: Option<String>,
    pub(crate) source_module: String,
    pub(crate) source_function: String,
    pub(crate) state: VmInspectedProcessState,
    pub(crate) mailbox_len: usize,
    pub(crate) reductions: u64,
    pub(crate) heap_bytes: u64,
    pub(crate) restart_count: u64,
    pub(crate) resource_handles: Vec<String>,
    pub(crate) native_call_state: Option<String>,
    pub(crate) cancellation_requested: bool,
}

/// Read-only supervisor snapshot exposed by VM inspection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmSupervisorInspectionSnapshot {
    pub(crate) id: String,
    pub(crate) strategy: String,
    pub(crate) child_pids: Vec<String>,
    pub(crate) restart_count: u64,
}

/// Read-only native/resource snapshot exposed by VM inspection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmResourceInspectionSnapshot {
    pub(crate) handle: String,
    pub(crate) owner_pid: String,
    pub(crate) kind: String,
    pub(crate) state: String,
}

/// Read-only timer snapshot exposed by VM inspection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmTimerInspectionSnapshot {
    pub(crate) id: String,
    pub(crate) owner_pid: String,
    pub(crate) remaining_ms: u64,
}

/// Read-only completed timer outcome exposed by VM inspection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmTimerOutcomeInspectionSnapshot {
    pub(crate) id: String,
    pub(crate) owner_pid: String,
    pub(crate) kind: String,
    pub(crate) outcome: String,
    pub(crate) detail: Option<String>,
}

/// Provider-bound runtime snapshot shared by CLI, TUI, and cloud inspection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmRuntimeInspectionSnapshot {
    pub(crate) provider_id: String,
    pub(crate) processes: Vec<VmProcessInspectionSnapshot>,
    pub(crate) supervisors: Vec<VmSupervisorInspectionSnapshot>,
    pub(crate) resources: Vec<VmResourceInspectionSnapshot>,
    pub(crate) timers: Vec<VmTimerInspectionSnapshot>,
    pub(crate) timer_outcomes: Vec<VmTimerOutcomeInspectionSnapshot>,
}

/// Stable read-only surface names required by VM inspection.
pub(crate) fn required_vm_runtime_inspection_surfaces() -> Vec<&'static str> {
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
}

/// Returns the default local VM instrumentation provider.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Provider using local VM scope and local-process transport.
///
/// Transformation:
/// - Defines the first TUI provider without requiring Terlan Cloud.
pub(crate) const fn default_local_vm_instrumentation_provider() -> VmInstrumentationProvider {
    VmInstrumentationProvider {
        id: "local.vm",
        display_name: "Local Terlan VM",
        scope: VmInstrumentationProviderScope::LocalVm,
        transport: VmInstrumentationTransport::LocalProcess,
    }
}

/// Returns the cloud instrumentation provider descriptor for shared UI tests.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Provider descriptor using cloud scope and cloud API transport.
///
/// Transformation:
/// - Defines the cloud-side provider shape without making the local VM TUI
///   depend on cloud connectivity.
pub(crate) const fn cloud_vm_instrumentation_provider() -> VmInstrumentationProvider {
    VmInstrumentationProvider {
        id: "cloud.vm",
        display_name: "Terlan Cloud VM",
        scope: VmInstrumentationProviderScope::TerlanCloud,
        transport: VmInstrumentationTransport::CloudApi,
    }
}

/// Returns provider-neutral VM dashboard components.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Stable ordered component list shared by local and cloud providers.
///
/// Transformation:
/// - Keeps component identity separate from provider transport so Ratatui and
///   future cloud dashboards can render the same logical UI.
pub(crate) fn standard_vm_dashboard_components() -> Vec<VmDashboardComponent> {
    vec![
        dashboard_component(
            "runtime",
            "Runtime",
            VmDashboardComponentKind::RuntimeOverview,
        ),
        dashboard_component(
            "processes",
            "Processes",
            VmDashboardComponentKind::ProcessList,
        ),
        dashboard_component("queues", "Queues", VmDashboardComponentKind::MessageQueues),
        dashboard_component(
            "native-boundary",
            "Native Boundary",
            VmDashboardComponentKind::NativeBoundary,
        ),
    ]
}

/// Returns the dashboard components available for one provider.
///
/// Inputs:
/// - `provider`: local or cloud VM instrumentation provider descriptor.
///
/// Output:
/// - Shared dashboard components when the provider has an explicit id.
/// - Stable diagnostic when provider identity is malformed.
///
/// Transformation:
/// - Validates provider identity but intentionally ignores provider transport
///   for component selection so local and cloud dashboards stay structurally
///   aligned.
pub(crate) fn vm_dashboard_components_for_provider(
    provider: &VmInstrumentationProvider,
) -> Result<Vec<VmDashboardComponent>, Vec<VmInstrumentationDiagnostic>> {
    if provider.id.trim().is_empty() {
        Err(vec![diagnostic(
            "vm_instrumentation_empty_provider_id",
            "VM instrumentation provider id must not be empty",
        )])
    } else {
        Ok(standard_vm_dashboard_components())
    }
}

/// Returns the default local VM dashboard configuration.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Read-only dashboard config for the local VM provider.
///
/// Transformation:
/// - Binds provider-neutral components to the local provider while keeping v1
///   inspection mode read-only.
pub(crate) fn default_local_vm_dashboard_config() -> VmDashboardConfig {
    let provider = default_local_vm_instrumentation_provider();
    VmDashboardConfig {
        mode: VmDashboardMode::ReadOnly,
        components: standard_vm_dashboard_components(),
        provider,
    }
}

/// Returns the default disabled operator policy.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Operator policy with no enabled actions and required audit semantics.
///
/// Transformation:
/// - Records the guard shape for later operator mode without enabling any
///   mutating UI controls in v1.
pub(crate) fn default_vm_operator_policy() -> VmOperatorPolicy {
    VmOperatorPolicy {
        enabled: false,
        audit_required: true,
        actions: Vec::new(),
    }
}

/// Returns the planned guarded operator action vocabulary.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Stable ordered action list for future guarded operator mode.
///
/// Transformation:
/// - Names planned mutating operations without making them executable from the
///   v1 dashboard.
pub(crate) fn planned_vm_operator_actions() -> Vec<VmOperatorAction> {
    vec![
        VmOperatorAction::HotReload,
        VmOperatorAction::Deploy,
        VmOperatorAction::Rollback,
        VmOperatorAction::NodeDrain,
        VmOperatorAction::ServiceRestart,
        VmOperatorAction::ReplicaPromotion,
    ]
}

/// Validates future VM operator policy.
///
/// Inputs:
/// - `policy`: guard policy for mutating dashboard operations.
///
/// Output:
/// - `Ok(())` only for the disabled v1 policy with audit required.
/// - Stable diagnostics when operator mode is enabled or audit is disabled.
///
/// Transformation:
/// - Keeps the operator vocabulary typed while ensuring v1 cannot expose
///   mutating actions.
pub(crate) fn validate_vm_operator_policy(
    policy: &VmOperatorPolicy,
) -> Result<(), Vec<VmInstrumentationDiagnostic>> {
    let mut diagnostics = Vec::new();
    if policy.enabled {
        diagnostics.push(diagnostic(
            "vm_operator_mode_not_available",
            "VM operator mode is planned but disabled for v1 dashboards",
        ));
    }
    if !policy.audit_required {
        diagnostics.push(diagnostic(
            "vm_operator_audit_required",
            "VM operator mode must require audit logging before it can be enabled",
        ));
    }

    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

/// Validates a VM dashboard configuration.
///
/// Inputs:
/// - `config`: dashboard mode, provider, and component set.
///
/// Output:
/// - `Ok(())` when v1 config is read-only, local-provider valid, and has
///   components.
/// - Stable diagnostics otherwise.
///
/// Transformation:
/// - Keeps the first terminal dashboard read-only until guarded operator
///   workflows have explicit policy and audit semantics.
pub(crate) fn validate_vm_dashboard_config(
    config: &VmDashboardConfig,
) -> Result<(), Vec<VmInstrumentationDiagnostic>> {
    let mut diagnostics = Vec::new();
    if config.mode != VmDashboardMode::ReadOnly {
        diagnostics.push(diagnostic(
            "vm_dashboard_operator_mode_disabled",
            "VM dashboard v1 is read-only; operator mode requires an explicit guard",
        ));
    }
    if let Err(provider_diagnostics) =
        validate_vm_instrumentation_providers(std::slice::from_ref(&config.provider))
    {
        diagnostics.extend(provider_diagnostics);
    }
    if config.components.is_empty() {
        diagnostics.push(diagnostic(
            "vm_dashboard_missing_components",
            "VM dashboard requires at least one read-only component",
        ));
    }

    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

/// Builds a text snapshot for a rendered local VM dashboard.
///
/// Inputs:
/// - `config`: local VM dashboard descriptor.
/// - `text`: terminal buffer text captured from the Ratatui renderer.
///
/// Output:
/// - Snapshot carrying provider/mode/component identity plus rendered text.
///
/// Transformation:
/// - Keeps tests and future CLI plumbing independent from Ratatui buffer
///   internals while proving the renderer consumes the same typed
///   instrumentation model as cloud/provider-neutral dashboards.
pub(crate) fn vm_dashboard_render_snapshot(
    config: &VmDashboardConfig,
    text: impl Into<String>,
) -> Result<VmDashboardRenderSnapshot, Vec<VmInstrumentationDiagnostic>> {
    validate_vm_dashboard_config(config)?;
    Ok(VmDashboardRenderSnapshot {
        provider_id: config.provider.id.to_string(),
        mode: config.mode.as_str(),
        component_ids: config
            .components
            .iter()
            .map(|component| component.id.to_string())
            .collect(),
        text: text.into(),
    })
}

/// Builds a read-only runtime inspection snapshot for one provider.
///
/// Inputs:
/// - `provider`: instrumentation provider that owns the snapshot source.
/// - `processes`: inspected VM process rows.
/// - `supervisors`: inspected supervisor rows.
/// - `resources`: inspected native/resource handle rows.
/// - `timers`: inspected timer rows.
/// - `timer_outcomes`: completed typed timer outcomes.
///
/// Output:
/// - Provider-bound inspection snapshot when provider identity is valid.
/// - Stable diagnostics for malformed local-provider configuration.
///
/// Transformation:
/// - Binds VM runtime state to the same provider model used by the TUI so
///   future `inspect` commands and cloud dashboards consume one typed shape
///   instead of scraping logs or depending on OTP terms.
pub(crate) fn vm_runtime_inspection_snapshot(
    provider: &VmInstrumentationProvider,
    processes: Vec<VmProcessInspectionSnapshot>,
    supervisors: Vec<VmSupervisorInspectionSnapshot>,
    resources: Vec<VmResourceInspectionSnapshot>,
    timers: Vec<VmTimerInspectionSnapshot>,
    timer_outcomes: Vec<VmTimerOutcomeInspectionSnapshot>,
) -> Result<VmRuntimeInspectionSnapshot, Vec<VmInstrumentationDiagnostic>> {
    validate_vm_instrumentation_providers(std::slice::from_ref(provider))?;
    Ok(VmRuntimeInspectionSnapshot {
        provider_id: provider.id.to_string(),
        processes,
        supervisors,
        resources,
        timers,
        timer_outcomes,
    })
}

/// Validates VM instrumentation providers.
///
/// Inputs:
/// - `providers`: provider declarations for local VM instrumentation.
///
/// Output:
/// - `Ok(())` when every provider is local-process scoped.
/// - Stable diagnostics for empty or non-local configuration.
///
/// Transformation:
/// - Rejects provider surfaces that would couple the local VM TUI to Terlan
///   Cloud or any remote transport.
pub(crate) fn validate_vm_instrumentation_providers(
    providers: &[VmInstrumentationProvider],
) -> Result<(), Vec<VmInstrumentationDiagnostic>> {
    let mut diagnostics = Vec::new();
    if providers.is_empty() {
        diagnostics.push(diagnostic(
            "vm_instrumentation_missing_provider",
            "VM instrumentation requires at least one local provider",
        ));
    }

    for provider in providers {
        if provider.id.trim().is_empty() {
            diagnostics.push(diagnostic(
                "vm_instrumentation_empty_provider_id",
                "VM instrumentation provider id must not be empty",
            ));
        }
        if provider.scope != VmInstrumentationProviderScope::LocalVm {
            diagnostics.push(diagnostic(
                "vm_instrumentation_non_local_scope",
                format!(
                    "VM instrumentation provider `{}` must use local_vm scope",
                    provider.id
                ),
            ));
        }
        if provider.transport != VmInstrumentationTransport::LocalProcess {
            diagnostics.push(diagnostic(
                "vm_instrumentation_non_local_transport",
                format!(
                    "VM instrumentation provider `{}` must use local_process transport",
                    provider.id
                ),
            ));
        }
    }

    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

/// Builds one dashboard component.
const fn dashboard_component(
    id: &'static str,
    title: &'static str,
    kind: VmDashboardComponentKind,
) -> VmDashboardComponent {
    VmDashboardComponent { id, title, kind }
}

/// Builds one VM instrumentation diagnostic.
fn diagnostic(code: &'static str, message: impl Into<String>) -> VmInstrumentationDiagnostic {
    VmInstrumentationDiagnostic {
        code,
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "instrumentation_test.rs"]
mod instrumentation_test;
