use super::*;
#[cfg(test)]
use crate::runtime::vm::table::VmTableEntry;

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

/// Acquired actor identity and placement, independent of response cookie policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VmHttpSessionLookup {
    pub(crate) session: VmHttpSession,
    pub(crate) route: VmHttpSessionRoute,
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

pub use terlan_http_native::session_registry::RecoveryPolicy as VmHttpSessionRecoveryPolicy;

use terlan_http_native::session_registry::{SessionEntry, SessionRegistry};

pub(super) type VmHttpSessionRecord = SessionEntry<VmHttpSessionResource>;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct VmHttpSessionResource {
    pub(super) actor: VmProcessId,
    pub(super) table: VmTableId,
    pub(super) state_version: u64,
    pub(super) command_results: BTreeMap<String, ReplValue>,
    pub(super) live_template_subscribers: BTreeMap<String, VmHttpSessionLiveTemplateSubscriber>,
}

/// VM actor/table adapter for the package-owned HTTP session registry.
///
/// Inputs:
/// - Source-selected identities, state reads/writes, rotation, and host ticks.
///
/// Output:
/// - Session resource handles, table state, routing metadata, and inspection rows.
///
/// Transformation:
/// - Delegates identity, lifetime, and recovery policy to std.http. Resource
///   mechanics reuse VM actors and tables; response cookies remain source-owned.
#[derive(Debug)]
pub struct VmHttpSessionRuntime {
    pub(super) actors: VmActorRuntime,
    pub(super) tables: VmTableStore,
    pub(super) sessions: SessionRegistry<VmHttpSessionResource>,
    node_id: String,
    ttl_ticks: u64,
    #[cfg(test)]
    pub(crate) live_template_protocol: VmLiveTemplateProtocolManifest,
}

#[path = "state/commands.rs"]
mod commands;
#[path = "state/runtime.rs"]
mod runtime;

#[path = "state/resources.rs"]
mod resources;

pub(crate) use commands::*;

#[path = "state/value_projection.rs"]
mod value_projection;
pub(crate) use value_projection::get;
