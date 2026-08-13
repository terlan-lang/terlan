//! Supervisor-owned logical-node listener and discovery lifecycle.

use std::net::{SocketAddr, TcpListener};
use std::num::NonZeroU16;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::lifecycle::{
    VmLogicalNodeEndpoint, VmLogicalNodeLifecycle, VmLogicalNodeLifecycleError,
    VmLogicalNodePhase,
};
use super::node_transport::{
    protocol_factory, VmNodePayloadDecoder, VmNodeTransportRouter,
};
use super::state::ConnectionId;
use super::transport::VmSharedEpmdState;
use super::super::fixed_scheduler_control::VmFixedSchedulerControl;
use super::super::protocol_task_executor::{
    start_protocol_tasks_with_topology, VmProtocolTaskServer,
};
use super::super::scheduler_topology::VmSchedulerTopology;

const LOGICAL_NODE_CONNECTION_NAMESPACE: u64 = 1_u64 << 63;
static NEXT_LOGICAL_NODE_CONNECTION: AtomicU64 =
    AtomicU64::new(LOGICAL_NODE_CONNECTION_NAMESPACE);

/// Production supervisor for one discoverable logical VM node.
pub(crate) struct VmLogicalNodeBootstrap<P: Send + 'static> {
    lifecycle: VmLogicalNodeLifecycle,
    router: Arc<VmNodeTransportRouter<P>>,
    server: Option<VmProtocolTaskServer>,
    epmd: VmSharedEpmdState,
}

impl<P: Send + 'static> VmLogicalNodeBootstrap<P> {
    /// Starts schedulers, listener, router, and exactly one EPMD registration.
    pub(crate) fn start(
        listener: TcpListener,
        name: Vec<u8>,
        extra: Vec<u8>,
        topology: VmSchedulerTopology,
        actors: Arc<VmFixedSchedulerControl<P>>,
        decoder: VmNodePayloadDecoder<P>,
        epmd: VmSharedEpmdState,
    ) -> Result<Self, String> {
        let listener_addr = listener
            .local_addr()
            .map_err(|error| format!("error[vm.logical_node.listener]: {error}"))?;
        let port = NonZeroU16::new(listener_addr.port()).ok_or_else(|| {
            "error[vm.logical_node.listener]: bound listener has port zero".to_string()
        })?;
        let endpoint = VmLogicalNodeEndpoint::new(name, port, extra)
            .map_err(render_lifecycle("create endpoint"))?;
        let mut lifecycle = VmLogicalNodeLifecycle::new(endpoint);
        let router = Arc::new(VmNodeTransportRouter::new(actors, decoder));
        let mut server = start_protocol_tasks_with_topology(
            listener,
            protocol_factory(Arc::clone(&router)),
            topology,
        )?;

        let startup = (|| {
            lifecycle
                .acknowledge_scheduler_pool_ready()
                .map_err(render_lifecycle("scheduler readiness"))?;
            lifecycle
                .acknowledge_listener_ready()
                .map_err(render_lifecycle("listener readiness"))?;
            router.open_admission();
            lifecycle
                .acknowledge_transport_router_ready()
                .map_err(render_lifecycle("router readiness"))?;
            let connection = next_connection_id()?;
            let mut state = epmd.lock().map_err(|_| {
                "error[vm.logical_node.epmd]: registry lock poisoned".to_string()
            })?;
            lifecycle
                .register(&mut state, connection)
                .map_err(render_lifecycle("register"))?;
            Ok(())
        })();

        match startup {
            Ok(()) => {}
            Err(error) => {
                router.close_admission();
                let cleanup = server.stop().err();
                return Err(append_cleanup(error, cleanup));
            }
        }
        Ok(Self {
            lifecycle,
            router,
            server: Some(server),
            epmd,
        })
    }

    /// Returns the ready logical-node listener address.
    pub(crate) fn transport_addr(&self) -> SocketAddr {
        self.server
            .as_ref()
            .expect("running bootstrap retains protocol server")
            .local_addr()
    }


    /// Returns the current logical-node lifecycle phase.
    pub(crate) const fn phase(&self) -> VmLogicalNodePhase {
        self.lifecycle.phase()
    }

    /// Returns whether this logical node still admits new transport messages.
    pub(crate) fn admits_transport(&self) -> bool {
        self.router.admits_transport() && self.lifecycle.admits_transport()
    }

    /// Stops admission and schedulers before withdrawing discovery.
    pub(crate) fn stop(&mut self) -> Result<(), String> {
        if self.lifecycle.phase() == VmLogicalNodePhase::Stopped {
            return Ok(());
        }
        self.router.close_admission();
        let mut first_error = self
            .lifecycle
            .close_admission()
            .map_err(render_lifecycle("close admission"))
            .err();
        if let Some(server) = self.server.as_mut() {
            if let Err(error) = server.stop() {
                first_error.get_or_insert(error);
            }
        }
        self.server.take();
        let unregister = self
            .epmd
            .lock()
            .map_err(|_| "error[vm.logical_node.epmd]: registry lock poisoned".to_string())
            .and_then(|mut state| {
                self.lifecycle
                    .unregister(&mut state)
                    .map_err(render_lifecycle("unregister"))
            });
        if let Err(error) = unregister {
            first_error.get_or_insert(error);
        } else if let Err(error) = self
            .lifecycle
            .acknowledge_stopped()
            .map_err(render_lifecycle("stop"))
        {
            first_error.get_or_insert(error);
        }
        first_error.map_or(Ok(()), Err)
    }
}

impl<P: Send + 'static> Drop for VmLogicalNodeBootstrap<P> {
    /// Applies fail-stop cleanup when the owning supervisor is dropped.
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// Allocates one nonzero registration identity for the node supervisor.
fn next_connection_id() -> Result<ConnectionId, String> {
    NEXT_LOGICAL_NODE_CONNECTION
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map(ConnectionId::new)
        .map_err(|_| "error[vm.logical_node.epmd]: connection identity exhausted".to_string())
}

/// Adds a stable startup stage to one lifecycle failure.
fn render_lifecycle(
    operation: &'static str,
) -> impl FnOnce(VmLogicalNodeLifecycleError) -> String {
    move |error| format!("error[vm.logical_node.{operation}]: {error}")
}

/// Preserves a primary startup failure while reporting cleanup failure.
fn append_cleanup(primary: String, cleanup: Option<String>) -> String {
    cleanup.map_or(primary.clone(), |cleanup| {
        format!("{primary}; error[vm.logical_node.cleanup]: {cleanup}")
    })
}
