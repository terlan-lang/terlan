//! Logical-node transport framing and fixed-owner message publication.

use std::future::Future;
use std::io::{self, Read, Write};
use std::num::NonZeroU64;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use super::super::fixed_scheduler_control::VmFixedSchedulerControl;
use super::super::protocol_task_executor::{
    VmProtocolTaskFactory, VmProtocolTaskRoute, VmReadyTcpStream,
};
use super::super::scheduler_topology::VmFixedActorRoute;

/// Maximum logical-node message body admitted before actor publication.
pub(crate) const VM_NODE_MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
const VM_NODE_ACTOR_BYTES: usize = size_of::<u64>();
const VM_NODE_STATUS_OK: u8 = 0;
const VM_NODE_STATUS_ERROR: u8 = 1;

/// Converts one validated node payload into the actor mailbox's owned type.
pub(crate) type VmNodePayloadDecoder<P> =
    Arc<dyn Fn(&[u8]) -> Result<P, String> + Send + Sync + 'static>;

/// Shared router that resolves ownership only when a complete message arrives.
pub(crate) struct VmNodeTransportRouter<P> {
    actors: Arc<VmFixedSchedulerControl<P>>,
    decoder: VmNodePayloadDecoder<P>,
    admitting: AtomicBool,
}

impl<P> VmNodeTransportRouter<P> {
    /// Creates a closed router over one shard-global actor directory.
    pub(crate) fn new(
        actors: Arc<VmFixedSchedulerControl<P>>,
        decoder: VmNodePayloadDecoder<P>,
    ) -> Self {
        Self {
            actors,
            decoder,
            admitting: AtomicBool::new(false),
        }
    }

    /// Opens message admission after every scheduler and listener is ready.
    pub(crate) fn open_admission(&self) {
        self.admitting.store(true, Ordering::Release);
    }

    /// Closes message admission before node discovery is withdrawn.
    pub(crate) fn close_admission(&self) {
        self.admitting.store(false, Ordering::Release);
    }

    /// Returns whether new complete transport messages may be published.
    pub(crate) fn admits_transport(&self) -> bool {
        self.admitting.load(Ordering::Acquire)
    }

    /// Resolves the actor's current owner and publishes one decoded payload.
    pub(crate) fn route(
        &self,
        actor_id: NonZeroU64,
        bytes: &[u8],
    ) -> Result<VmFixedActorRoute, String> {
        if !self.admits_transport() {
            return Err("error[vm.node_transport.closed]: node admission is closed".to_string());
        }
        let route = self.actors.resolve_route(actor_id)?;
        let payload = (self.decoder)(bytes)?;
        self.actors.publish(route, payload)?;
        Ok(route)
    }
}

/// Encodes one length-prefixed actor message for the logical-node transport.
#[cfg(test)]
pub(crate) fn encode_node_transport_frame(
    actor_id: NonZeroU64,
    payload: &[u8],
) -> Result<Vec<u8>, String> {
    let body_len = VM_NODE_ACTOR_BYTES
        .checked_add(payload.len())
        .ok_or_else(|| "error[vm.node_transport.frame]: body length overflow".to_string())?;
    if body_len > VM_NODE_MAX_FRAME_BYTES {
        return Err(format!(
            "error[vm.node_transport.frame]: body exceeds {VM_NODE_MAX_FRAME_BYTES} bytes"
        ));
    }
    let body_len = u32::try_from(body_len)
        .map_err(|_| "error[vm.node_transport.frame]: body length exceeds UInt32".to_string())?;
    let mut frame = Vec::with_capacity(size_of::<u32>() + body_len as usize);
    frame.extend_from_slice(&body_len.to_be_bytes());
    frame.extend_from_slice(&actor_id.get().to_be_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

/// Builds a protocol future factory for one logical-node transport router.
pub(crate) fn protocol_factory<P: Send + 'static>(
    router: Arc<VmNodeTransportRouter<P>>,
) -> VmProtocolTaskFactory {
    Arc::new(move |stream, route| {
        Box::pin(VmNodeTransportConnection::new(
            stream,
            route,
            Arc::clone(&router),
        ))
    })
}

/// Incremental state for one bounded logical-node request.
enum VmNodeTransportPhase {
    /// Reading the four-byte body length.
    ReadingLength { bytes: [u8; 4], offset: usize },
    /// Reading actor identity and payload.
    ReadingBody { bytes: Vec<u8>, offset: usize },
    /// Writing a terminal acknowledgement.
    Writing { bytes: Vec<u8>, offset: usize },
    /// The request has completed.
    Complete,
}

/// One logical-node socket future pinned to a fixed protocol owner.
struct VmNodeTransportConnection<P> {
    stream: VmReadyTcpStream,
    route: VmProtocolTaskRoute,
    router: Arc<VmNodeTransportRouter<P>>,
    phase: VmNodeTransportPhase,
}

impl<P> VmNodeTransportConnection<P> {
    /// Creates one connection that admits exactly one bounded actor message.
    fn new(
        stream: VmReadyTcpStream,
        route: VmProtocolTaskRoute,
        router: Arc<VmNodeTransportRouter<P>>,
    ) -> Self {
        Self {
            stream,
            route,
            router,
            phase: VmNodeTransportPhase::ReadingLength {
                bytes: [0; 4],
                offset: 0,
            },
        }
    }

    /// Advances framing, owner resolution, publication, and acknowledgement.
    fn poll_connection(&mut self) -> Poll<Result<(), String>> {
        loop {
            match &mut self.phase {
                VmNodeTransportPhase::ReadingLength { bytes, offset } => {
                    match read_available(&mut self.stream, bytes, offset)? {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(()) => {
                            let body_len = u32::from_be_bytes(*bytes) as usize;
                            if !(VM_NODE_ACTOR_BYTES..=VM_NODE_MAX_FRAME_BYTES).contains(&body_len) {
                                self.phase = VmNodeTransportPhase::Writing {
                                    bytes: error_ack("invalid body length"),
                                    offset: 0,
                                };
                            } else {
                                self.phase = VmNodeTransportPhase::ReadingBody {
                                    bytes: vec![0; body_len],
                                    offset: 0,
                                };
                            }
                        }
                    }
                }
                VmNodeTransportPhase::ReadingBody { bytes, offset } => {
                    match read_available(&mut self.stream, bytes, offset)? {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(()) => {
                            let actor = u64::from_be_bytes(
                                bytes[..VM_NODE_ACTOR_BYTES]
                                    .try_into()
                                    .expect("validated actor prefix"),
                            );
                            let result = NonZeroU64::new(actor)
                                .ok_or_else(|| {
                                    "error[vm.node_transport.actor]: actor identity is zero"
                                        .to_string()
                                })
                                .and_then(|actor| {
                                    self.router
                                        .route(actor, &bytes[VM_NODE_ACTOR_BYTES..])
                                });
                            self.phase = VmNodeTransportPhase::Writing {
                                bytes: result.map_or_else(
                                    |error| error_ack(&error),
                                    success_ack,
                                ),
                                offset: 0,
                            };
                        }
                    }
                }
                VmNodeTransportPhase::Writing { bytes, offset } => {
                    match write_available(&mut self.stream, bytes, offset)? {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(()) => {
                            self.phase = VmNodeTransportPhase::Complete;
                        }
                    }
                }
                VmNodeTransportPhase::Complete => return Poll::Ready(Ok(())),
            }
        }
    }
}

impl<P> Future for VmNodeTransportConnection<P> {
    /// Terminal connection result after publication acknowledgement.
    type Output = Result<(), String>;

    /// Polls the connection only under its exact fixed protocol owner.
    fn poll(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        if let Err(error) = self.route.validate_completion_origin() {
            return Poll::Ready(Err(error));
        }
        self.poll_connection()
    }
}

/// Reads a fixed buffer until completion or readiness exhaustion.
fn read_available(
    stream: &mut impl Read,
    bytes: &mut [u8],
    offset: &mut usize,
) -> Result<Poll<()>, String> {
    while *offset < bytes.len() {
        match stream.read(&mut bytes[*offset..]) {
            Ok(0) => {
                return Err("error[vm.node_transport.io]: unexpected end of request".to_string())
            }
            Ok(read) => *offset += read,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(Poll::Pending),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(format!("error[vm.node_transport.io]: read: {error}")),
        }
    }
    Ok(Poll::Ready(()))
}

/// Writes a fixed response until completion or readiness exhaustion.
fn write_available(
    stream: &mut impl Write,
    bytes: &[u8],
    offset: &mut usize,
) -> Result<Poll<()>, String> {
    while *offset < bytes.len() {
        match stream.write(&bytes[*offset..]) {
            Ok(0) => {
                return Err(
                    "error[vm.node_transport.io]: response write made no progress".to_string(),
                )
            }
            Ok(written) => *offset += written,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(Poll::Pending),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(format!("error[vm.node_transport.io]: write: {error}")),
        }
    }
    Ok(Poll::Ready(()))
}

/// Encodes a successful publication acknowledgement and resolved owner.
fn success_ack(route: VmFixedActorRoute) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(1 + size_of::<u32>());
    bytes.push(VM_NODE_STATUS_OK);
    bytes.extend_from_slice(&(route.scheduler().index() as u32).to_be_bytes());
    bytes
}

/// Encodes a bounded terminal transport error.
fn error_ack(error: &str) -> Vec<u8> {
    let message = error.as_bytes();
    let message = &message[..message.len().min(u16::MAX as usize)];
    let mut bytes = Vec::with_capacity(1 + size_of::<u16>() + message.len());
    bytes.push(VM_NODE_STATUS_ERROR);
    bytes.extend_from_slice(&(message.len() as u16).to_be_bytes());
    bytes.extend_from_slice(message);
    bytes
}
