use super::*;
#[cfg(test)]
use crate::runtime::vm::table::VmTableEntry;

const SESSION_COOKIE_NAME: &str = "terlan_session";

/// HTTP session actor handle exposed to the HTTP runtime boundary.
/// Inputs:
/// - Stable session id allocated by the VM session runtime.
///
/// Output:
/// - Opaque handle used by request handlers and response cookie threading.
///
/// Transformation:
/// - Keeps HTTP cookies separate from VM process/table identity while still
///   allowing deterministic lookup and sticky routing metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VmHttpSession {
    pub(crate) id: String,
}

impl VmHttpSession {
    /// Reconstitutes an opaque handle from managed request-context state.
    pub(crate) fn from_managed_id(id: String) -> Self {
        Self { id }
    }

    /// Returns the stable public identity carried by managed session values.
    pub(crate) fn managed_id(&self) -> &str {
        &self.id
    }
}

/// Sticky-session routing metadata.
/// Inputs:
/// - VM node id, session id, and owning actor pid.
///
/// Output:
/// - Deployment-facing metadata that a load balancer or Terlan Cloud router can
///   use as an optimization.
///
/// Transformation:
/// - Describes actor placement without making correctness depend on sticky
///   routing. Recovery remains an explicit policy above this runtime layer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VmHttpSessionRoute {
    pub(crate) node_id: String,
    pub(crate) session_id: String,
    pub(crate) actor_pid: u64,
    pub(crate) sticky_key: String,
}

/// Typed session-affinity request emitted by a route, middleware, or handler.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub struct VmHttpSessionAffinityKey {
    pub(crate) source: String,
    pub(crate) key: String,
}

#[cfg(test)]
impl VmHttpSessionAffinityKey {
    pub(crate) fn new(source: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            key: key.into(),
        }
    }
}

/// Rejected stateful HTTP actor affinity decision.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub enum VmHttpSessionAffinityError {
    MissingAffinityKey,
    ConflictingAffinityKeys {
        existing_source: String,
        existing_key: String,
        incoming_source: String,
        incoming_key: String,
    },
}

/// Result of cookie-to-session lookup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VmHttpSessionLookup {
    pub(crate) session: VmHttpSession,
    pub(crate) route: VmHttpSessionRoute,
    pub(crate) set_cookie_header: Option<String>,
}

impl VmHttpSessionLookup {
    /// Splits a lookup into its opaque handle and pending response cookie.
    pub(crate) fn into_managed_parts(self) -> (VmHttpSession, Option<String>) {
        (self.session, self.set_cookie_header)
    }
}

/// Idempotent stateful HTTP command result.
#[derive(Clone, Debug, PartialEq)]
#[cfg(test)]
pub enum VmHttpSessionCommandOutcome {
    Applied(ReplValue),
    Replayed(ReplValue),
}

/// Durable state replay payload for one HTTP session actor.
#[derive(Clone, Debug, PartialEq)]
#[cfg(test)]
pub struct VmHttpSessionPersistenceSnapshot {
    pub(crate) session_id: String,
    pub(crate) expires_at_tick: u64,
    pub(crate) state_version: u64,
    pub(crate) table_entries: Vec<VmTableEntry>,
    pub(crate) command_results: BTreeMap<String, ReplValue>,
}

/// Runtime-attributed mailbox pressure for one stateful HTTP session actor.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub struct VmHttpSessionMailboxBackpressure {
    pub(crate) session_id: String,
    pub(crate) actor_pid: u64,
    pub(crate) mailbox_len: usize,
    pub(crate) threshold: usize,
    pub(crate) saturated: bool,
    pub(crate) attribution: String,
}

/// Completed stateful HTTP session migration between VM workers.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub struct VmHttpSessionWorkerMigration {
    pub(crate) session_id: String,
    pub(crate) source_route: VmHttpSessionRoute,
    pub(crate) destination_route: VmHttpSessionRoute,
    pub(crate) set_cookie_header: Option<String>,
    pub(crate) diagnostic: String,
}

/// Stateful HTTP session compatibility row for one VM hot reload.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub struct VmHttpSessionHotReloadMigrationReport {
    pub(crate) session_id: String,
    pub(crate) previous_generation: u64,
    pub(crate) active_generation: u64,
    pub(crate) compatible: bool,
    pub(crate) durable_table_entries: usize,
    pub(crate) durable_command_results: usize,
    pub(crate) transient_subscribers: usize,
    pub(crate) diagnostic: String,
}

/// Live-template subscriber owned by one HTTP session actor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VmHttpSessionLiveTemplateSubscriber {
    pub(crate) id: String,
    pub(crate) transport: String,
}

/// Capability-checked live-template subscription admission result.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub struct VmHttpSessionLiveTemplateSubscriptionAuthorization {
    pub(crate) subscriber: VmHttpSessionLiveTemplateSubscriber,
    pub(crate) required_capability: String,
    pub(crate) granted_capabilities: Vec<String>,
    pub(crate) diagnostic: String,
}

/// Typed live-template binding to one stateful VM actor table slot.
#[derive(Clone, Debug, PartialEq)]
#[cfg(test)]
pub struct VmHttpSessionLiveTemplateActorBinding {
    pub(crate) session_id: String,
    pub(crate) actor_pid: u64,
    pub(crate) table_id: u64,
    pub(crate) template_id: String,
    pub(crate) state_key: String,
    pub(crate) state_value: Option<ReplValue>,
    pub(crate) state_version: u64,
    pub(crate) live_template_subscriber_count: usize,
    pub(crate) diagnostic: String,
}

/// Source-map-aware trace for a live-template subscription.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub struct VmHttpSessionLiveTemplateSubscriptionTrace {
    pub(crate) session_id: String,
    pub(crate) actor_pid: u64,
    pub(crate) subscriber_id: String,
    pub(crate) transport: String,
    pub(crate) template_id: String,
    pub(crate) source_module: String,
    pub(crate) source_line: u32,
    pub(crate) source_column: u32,
    pub(crate) state_version: u64,
    pub(crate) diagnostic: String,
}

/// One live-template patch event targeted at a subscriber.
#[derive(Clone, Debug, PartialEq)]
#[cfg(test)]
pub struct VmHttpSessionLiveTemplateFanoutEvent {
    pub(crate) subscriber_id: String,
    pub(crate) transport: String,
    pub(crate) event_id: String,
    pub(crate) event_name: String,
    pub(crate) payload: ReplValue,
}

/// Cross-template fanout result for one actor state update.
#[derive(Clone, Debug, PartialEq)]
#[cfg(test)]
pub struct VmHttpSessionLiveTemplateStateFanout {
    pub(crate) session_id: String,
    pub(crate) state_version: u64,
    pub(crate) patch_event: String,
    pub(crate) subscriber_events: Vec<VmHttpSessionLiveTemplateFanoutEvent>,
}

/// Runtime-inspection row for an HTTP session actor.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub struct VmHttpSessionSnapshot {
    pub(crate) session_id: String,
    pub(crate) actor_pid: u64,
    pub(crate) table_id: u64,
    pub(crate) table_len: usize,
    pub(crate) live_template_subscriber_count: usize,
    pub(crate) actor_mailbox_len: usize,
    pub(crate) state_version: u64,
    pub(crate) expires_at_tick: u64,
    pub(crate) sticky_key: String,
}

/// Session recovery behavior for stale or expired cookie ids.
///
/// Inputs:
/// - Selected by the VM HTTP runtime or deployment profile.
///
/// Output:
/// - Deterministic stale-session behavior.
///
/// Transformation:
/// - Makes recovery policy explicit so correctness never silently depends on
///   load-balancer stickiness. Distributed and persistent recovery modes can
///   extend this enum without changing handler-facing session calls.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VmHttpSessionRecoveryPolicy {
    CreateLocalReplacement,
    FailClosed,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct VmHttpSessionRecord {
    pub(super) id: String,
    pub(super) actor: VmProcessId,
    pub(super) table: VmTableId,
    pub(super) expires_at_tick: u64,
    pub(super) state_version: u64,
    pub(super) command_results: BTreeMap<String, ReplValue>,
    pub(super) live_template_subscribers: BTreeMap<String, VmHttpSessionLiveTemplateSubscriber>,
}

/// VM-owned HTTP session runtime.
///
/// Inputs:
/// - Request cookie session ids, session state reads/writes, rotation, and
///   expiration ticks.
///
/// Output:
/// - Session actors, VM-owned table state, response cookie headers, sticky
///   routing metadata, and inspection rows.
///
/// Transformation:
/// - Reuses the VM actor runtime for process identity and the VM table store
///   for state. The HTTP layer only binds cookies to session ids; it does not
///   own hidden maps or a parallel session subsystem.
#[derive(Debug)]
pub struct VmHttpSessionRuntime {
    pub(super) actors: VmActorRuntime,
    pub(super) tables: VmTableStore,
    pub(super) sessions: BTreeMap<String, VmHttpSessionRecord>,
    next_session_id: u64,
    now_tick: u64,
    ttl_ticks: u64,
    node_id: String,
    recovery_policy: VmHttpSessionRecoveryPolicy,
    #[cfg(test)]
    pub(crate) live_template_protocol: VmLiveTemplateProtocolManifest,
}

#[path = "state/commands.rs"]
mod commands;
#[path = "state/runtime.rs"]
mod runtime;

pub(crate) use commands::*;
pub(crate) use runtime::*;
