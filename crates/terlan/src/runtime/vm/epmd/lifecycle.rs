//! Supervisor-owned lifecycle for one discoverable logical VM node.

use std::fmt;
use std::num::NonZeroU16;
use std::num::NonZeroU64;

use super::super::fixed_scheduler_control::VmFixedSchedulerControl;
use super::super::scheduler_topology::VmFixedActorRoute;
use super::protocol::{validate_extra, validate_name, Alive2Request, RegistrationResult};
use super::state::{ConnectionId, ServerState};

/// One logical node endpoint advertised through EPMD.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmLogicalNodeEndpoint {
    name: Vec<u8>,
    transport_port: NonZeroU16,
    node_type: u8,
    protocol: u8,
    highest_version: u16,
    lowest_version: u16,
    extra: Vec<u8>,
}

impl VmLogicalNodeEndpoint {
    /// Creates an OTP-compatible endpoint for one Terlan logical node.
    pub(crate) fn new(
        name: impl Into<Vec<u8>>,
        transport_port: NonZeroU16,
        extra: impl Into<Vec<u8>>,
    ) -> Result<Self, VmLogicalNodeLifecycleError> {
        let name = name.into();
        let extra = extra.into();
        validate_name(&name).map_err(|_| VmLogicalNodeLifecycleError::InvalidNodeName)?;
        validate_extra(&extra).map_err(|_| VmLogicalNodeLifecycleError::InvalidExtraData)?;
        Ok(Self {
            name,
            transport_port,
            node_type: 77,
            protocol: 0,
            highest_version: 6,
            lowest_version: 5,
            extra,
        })
    }

    /// Returns the registered node name bytes.
    pub(crate) fn name(&self) -> &[u8] {
        &self.name
    }

    /// Returns the logical node transport port.
    pub(crate) const fn transport_port(&self) -> u16 {
        self.transport_port.get()
    }

    /// Builds the canonical ALIVE2 request for this endpoint.
    fn alive2_request(&self) -> Alive2Request {
        Alive2Request {
            port: self.transport_port(),
            node_type: self.node_type,
            protocol: self.protocol,
            highest_version: self.highest_version,
            lowest_version: self.lowest_version,
            name: self.name.clone(),
            extra: self.extra.clone(),
        }
    }
}

/// Observable startup and shutdown phase for one logical node.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmLogicalNodePhase {
    /// No runtime component has reported readiness.
    Created,
    /// Every configured scheduler owner is ready.
    SchedulerPoolReady,
    /// The node transport listener is bound and accepting readiness events.
    ListenerReady,
    /// The transport router can resolve current actor owners.
    RouterReady,
    /// One EPMD registration owns the logical endpoint.
    Registered,
    /// New node transport admission is closed while accepted work drains.
    AdmissionClosed,
    /// The EPMD registration has been removed.
    Unregistered,
    /// All node-owned runtime components have stopped.
    Stopped,
}

/// Typed rejection from the logical-node lifecycle state machine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmLogicalNodeLifecycleError {
    /// The endpoint node name violates EPMD constraints.
    InvalidNodeName,
    /// The endpoint extra payload violates EPMD constraints.
    InvalidExtraData,
    /// An operation was attempted outside its required lifecycle phase.
    InvalidTransition {
        /// Phase observed before the rejected operation.
        phase: VmLogicalNodePhase,
        /// Stable operation name.
        operation: &'static str,
    },
    /// EPMD rejected the logical endpoint registration.
    RegistrationRejected,
    /// The registration disappeared or belongs to another connection.
    RegistrationOwnershipLost,
    /// Incoming node work could not resolve a live actor owner.
    TransportRoute(String),
}

impl fmt::Display for VmLogicalNodeLifecycleError {
    /// Renders a stable lifecycle diagnostic.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidNodeName => formatter.write_str("invalid EPMD node name"),
            Self::InvalidExtraData => formatter.write_str("invalid EPMD extra data"),
            Self::InvalidTransition { phase, operation } => {
                write!(formatter, "operation `{operation}` is invalid during {phase:?}")
            }
            Self::RegistrationRejected => formatter.write_str("EPMD registration was rejected"),
            Self::RegistrationOwnershipLost => {
                formatter.write_str("EPMD registration ownership was lost")
            }
            Self::TransportRoute(reason) => formatter.write_str(reason),
        }
    }
}

impl std::error::Error for VmLogicalNodeLifecycleError {}

/// Supervisor-owned readiness, registration, admission, and shutdown state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmLogicalNodeLifecycle {
    endpoint: VmLogicalNodeEndpoint,
    phase: VmLogicalNodePhase,
    registration: Option<ConnectionId>,
    creation: Option<u32>,
}

impl VmLogicalNodeLifecycle {
    /// Creates one dormant logical node under its shard supervisor.
    pub(crate) fn new(endpoint: VmLogicalNodeEndpoint) -> Self {
        Self {
            endpoint,
            phase: VmLogicalNodePhase::Created,
            registration: None,
            creation: None,
        }
    }

    /// Returns the current logical-node lifecycle phase.
    pub(crate) const fn phase(&self) -> VmLogicalNodePhase {
        self.phase
    }

    /// Returns the endpoint owned by this logical node.
    pub(crate) const fn endpoint(&self) -> &VmLogicalNodeEndpoint {
        &self.endpoint
    }

    /// Returns the EPMD creation value for the live registration.
    pub(crate) const fn creation(&self) -> Option<u32> {
        self.creation
    }

    /// Records readiness of the complete scheduler pool, not one scheduler.
    pub(crate) fn acknowledge_scheduler_pool_ready(
        &mut self,
    ) -> Result<(), VmLogicalNodeLifecycleError> {
        self.advance(
            VmLogicalNodePhase::Created,
            VmLogicalNodePhase::SchedulerPoolReady,
            "acknowledge_scheduler_pool_ready",
        )
    }

    /// Records that the logical node listener is bound and ready.
    pub(crate) fn acknowledge_listener_ready(
        &mut self,
    ) -> Result<(), VmLogicalNodeLifecycleError> {
        self.advance(
            VmLogicalNodePhase::SchedulerPoolReady,
            VmLogicalNodePhase::ListenerReady,
            "acknowledge_listener_ready",
        )
    }

    /// Records that incoming node work can resolve current actor owners.
    pub(crate) fn acknowledge_transport_router_ready(
        &mut self,
    ) -> Result<(), VmLogicalNodeLifecycleError> {
        self.advance(
            VmLogicalNodePhase::ListenerReady,
            VmLogicalNodePhase::RouterReady,
            "acknowledge_transport_router_ready",
        )
    }

    /// Registers exactly one ready logical endpoint in EPMD.
    pub(crate) fn register(
        &mut self,
        epmd: &mut ServerState,
        connection: ConnectionId,
    ) -> Result<u32, VmLogicalNodeLifecycleError> {
        self.require_phase(VmLogicalNodePhase::RouterReady, "register")?;
        let attempt = epmd.register_alive2(connection, &self.endpoint.alive2_request());
        if attempt.result != RegistrationResult::Ok || !attempt.registered {
            return Err(VmLogicalNodeLifecycleError::RegistrationRejected);
        }
        self.registration = Some(connection);
        self.creation = Some(attempt.creation);
        self.phase = VmLogicalNodePhase::Registered;
        Ok(attempt.creation)
    }

    /// Returns whether incoming node transport admission is open.
    pub(crate) const fn admits_transport(&self) -> bool {
        matches!(self.phase, VmLogicalNodePhase::Registered)
    }

    /// Routes incoming node work to the actor's current scheduler owner.
    pub(crate) fn route_incoming<P>(
        &self,
        actors: &VmFixedSchedulerControl<P>,
        actor_id: NonZeroU64,
    ) -> Result<VmFixedActorRoute, VmLogicalNodeLifecycleError> {
        if !self.admits_transport() {
            return Err(VmLogicalNodeLifecycleError::InvalidTransition {
                phase: self.phase,
                operation: "route_incoming",
            });
        }
        actors
            .resolve_route(actor_id)
            .map_err(VmLogicalNodeLifecycleError::TransportRoute)
    }

    /// Stops new node transport admission before registration removal.
    pub(crate) fn close_admission(&mut self) -> Result<(), VmLogicalNodeLifecycleError> {
        self.advance(
            VmLogicalNodePhase::Registered,
            VmLogicalNodePhase::AdmissionClosed,
            "close_admission",
        )
    }

    /// Removes the exact registration after admission has stopped.
    pub(crate) fn unregister(
        &mut self,
        epmd: &mut ServerState,
    ) -> Result<(), VmLogicalNodeLifecycleError> {
        self.require_phase(VmLogicalNodePhase::AdmissionClosed, "unregister")?;
        let connection = self
            .registration
            .ok_or(VmLogicalNodeLifecycleError::RegistrationOwnershipLost)?;
        let removed = epmd
            .unregister_connection(connection)
            .ok_or(VmLogicalNodeLifecycleError::RegistrationOwnershipLost)?;
        if removed.name.as_slice() != self.endpoint.name() {
            return Err(VmLogicalNodeLifecycleError::RegistrationOwnershipLost);
        }
        self.registration = None;
        self.creation = None;
        self.phase = VmLogicalNodePhase::Unregistered;
        Ok(())
    }

    /// Marks node-owned components stopped after registration removal.
    pub(crate) fn acknowledge_stopped(&mut self) -> Result<(), VmLogicalNodeLifecycleError> {
        self.advance(
            VmLogicalNodePhase::Unregistered,
            VmLogicalNodePhase::Stopped,
            "acknowledge_stopped",
        )
    }

    /// Executes fail-stop ordering without depending on scheduler availability.
    pub(crate) fn fail_stop(
        &mut self,
        epmd: &mut ServerState,
    ) -> Result<(), VmLogicalNodeLifecycleError> {
        self.close_admission()?;
        self.unregister(epmd)?;
        self.acknowledge_stopped()
    }

    /// Advances one exact phase transition.
    fn advance(
        &mut self,
        expected: VmLogicalNodePhase,
        next: VmLogicalNodePhase,
        operation: &'static str,
    ) -> Result<(), VmLogicalNodeLifecycleError> {
        self.require_phase(expected, operation)?;
        self.phase = next;
        Ok(())
    }

    /// Rejects an operation unless the exact expected phase is active.
    fn require_phase(
        &self,
        expected: VmLogicalNodePhase,
        operation: &'static str,
    ) -> Result<(), VmLogicalNodeLifecycleError> {
        if self.phase == expected {
            return Ok(());
        }
        Err(VmLogicalNodeLifecycleError::InvalidTransition {
            phase: self.phase,
            operation,
        })
    }
}
