//! EPMD discovery and logical-node lifecycle tests.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::num::{NonZeroU16, NonZeroU64};
use std::sync::Arc;
use std::time::Duration;

use super::bootstrap::VmLogicalNodeBootstrap;
use super::client::{
    kill_client_output, names_like_client_output, names_request_frame, stop_request_frame,
};
use super::lifecycle::{
    VmLogicalNodeEndpoint, VmLogicalNodeLifecycle, VmLogicalNodeLifecycleError,
    VmLogicalNodePhase,
};
use super::protocol::{
    encode_frame, encode_port2_response, parse_frame, parse_payload, Alive2Request, NameRequest,
    Port2Response, ProtocolError, Request, ALIVE2_REQ, NAMES_REQ, PORT2_REQ, PORT2_RESP,
};
use super::state::{ConnectionId, ServerOptions, ServerState};
use super::transport::{handle_payload, shared_state};
use super::node_transport::{
    encode_node_transport_frame, VmNodePayloadDecoder, VmNodeTransportRouter,
    VM_NODE_MAX_FRAME_BYTES,
};
use super::super::fixed_scheduler_control::VmFixedSchedulerControl;
use super::super::protocol_task_executor::{
    bind_protocol_listener, start_protocol_tasks_with_topology,
};
use super::super::scheduler_topology::{VmSchedulerId, VmSchedulerTopology};
use super::super::actor_directory::VmActorLifecycle;

/// Builds one valid logical endpoint used by lifecycle tests.
fn endpoint(name: &str, port: u16) -> VmLogicalNodeEndpoint {
    VmLogicalNodeEndpoint::new(
        name.as_bytes().to_vec(),
        NonZeroU16::new(port).expect("test endpoint port"),
        b"terlan".to_vec(),
    )
    .expect("valid test endpoint")
}

/// Advances one logical node through every startup readiness barrier.
fn ready_node(name: &str, port: u16) -> VmLogicalNodeLifecycle {
    let mut node = VmLogicalNodeLifecycle::new(endpoint(name, port));
    node.acknowledge_scheduler_pool_ready()
        .expect("scheduler pool ready");
    node.acknowledge_listener_ready().expect("listener ready");
    node.acknowledge_transport_router_ready()
        .expect("router ready");
    node
}

/// Proves valid ALIVE2 round-trip and malformed frame rejection.
#[test]
fn epmd_protocol_round_trips_alive2_and_rejects_malformed_frames() {
    let request = Alive2Request {
        port: 4040,
        node_type: 77,
        protocol: 0,
        highest_version: 6,
        lowest_version: 5,
        name: b"node@host".to_vec(),
        extra: b"terlan".to_vec(),
    };
    let mut payload = vec![ALIVE2_REQ];
    payload.extend_from_slice(&request.port.to_be_bytes());
    payload.push(request.node_type);
    payload.push(request.protocol);
    payload.extend_from_slice(&request.highest_version.to_be_bytes());
    payload.extend_from_slice(&request.lowest_version.to_be_bytes());
    payload.extend_from_slice(&(request.name.len() as u16).to_be_bytes());
    payload.extend_from_slice(&request.name);
    payload.extend_from_slice(&(request.extra.len() as u16).to_be_bytes());
    payload.extend_from_slice(&request.extra);

    let frame = encode_frame(&payload).expect("encode alive frame");
    assert_eq!(parse_frame(&frame), Ok(Request::Alive2(request)));
    assert_eq!(parse_frame(&[]), Err(ProtocolError::Incomplete));
    assert_eq!(parse_frame(&[0, 2, NAMES_REQ]), Err(ProtocolError::LengthMismatch));
    assert_eq!(parse_payload(&[0xff]), Err(ProtocolError::UnknownTag(0xff)));
}

/// Proves invalid node names cannot enter lookup response encoding.
#[test]
fn epmd_protocol_rejects_invalid_names_and_response_fields() {
    assert_eq!(
        parse_payload(&[b'z']),
        Err(ProtocolError::EmptyName)
    );
    assert_eq!(
        parse_payload(&[b'z', b'a', 0, b'b']),
        Err(ProtocolError::NameContainsNul)
    );
    assert_eq!(
        encode_port2_response(&Port2Response::Found(
            super::protocol::Port2Found {
                port: 1,
                node_type: 77,
                protocol: 0,
                highest_version: 6,
                lowest_version: 5,
                name: Vec::new(),
                extra: Vec::new(),
            }
        )),
        Err(ProtocolError::EmptyName)
    );
}

/// Proves exact connection ownership controls registration lifetime.
#[test]
fn epmd_registry_owns_registration_until_exact_connection_closes() {
    let mut state = ServerState::new(ServerOptions::new(4369));
    let request = Alive2Request {
        port: 4040,
        node_type: 77,
        protocol: 0,
        highest_version: 6,
        lowest_version: 5,
        name: b"node@host".to_vec(),
        extra: Vec::new(),
    };
    let first = state.register_alive2(ConnectionId::new(1), &request);
    let duplicate = state.register_alive2(ConnectionId::new(2), &request);

    assert!(first.registered);
    assert!(!duplicate.registered);
    assert_eq!(state.registered_len(), 1);
    assert!(state.unregister_connection(ConnectionId::new(2)).is_none());
    assert!(state.lookup(b"node@host").is_some());
    assert!(state.unregister_connection(ConnectionId::new(1)).is_some());
    assert!(state.lookup(b"node@host").is_none());
}

/// Proves registration cannot precede any logical-node readiness barrier.
#[test]
fn logical_node_registers_only_after_pool_listener_and_router_are_ready() {
    let mut state = ServerState::new(ServerOptions::new(4369));
    let mut node = VmLogicalNodeLifecycle::new(endpoint("ready@host", 8081));
    assert!(matches!(
        node.register(&mut state, ConnectionId::new(10)),
        Err(VmLogicalNodeLifecycleError::InvalidTransition {
            phase: VmLogicalNodePhase::Created,
            operation: "register"
        })
    ));
    node.acknowledge_scheduler_pool_ready().expect("pool ready");
    node.acknowledge_listener_ready().expect("listener ready");
    node.acknowledge_transport_router_ready()
        .expect("router ready");
    let creation = node
        .register(&mut state, ConnectionId::new(10))
        .expect("register ready node");

    assert_eq!(node.phase(), VmLogicalNodePhase::Registered);
    assert_eq!(node.creation(), Some(creation));
    assert_eq!(node.endpoint().transport_port(), 8081);
    assert_eq!(state.registered_len(), 1);
}

/// Proves actor migration changes routing without changing EPMD registration.
#[test]
fn one_logical_registration_survives_scheduler_owner_migration() {
    let topology = VmSchedulerTopology::new(2).expect("two schedulers");
    let actors = VmFixedSchedulerControl::<()>::default();
    let actor_id = NonZeroU64::new(2).expect("actor id");
    let route = topology.route(actor_id);
    actors.register(route).expect("register actor route");
    let mut state = ServerState::new(ServerOptions::new(4369));
    let mut node = ready_node("stable@host", 8082);
    node.register(&mut state, ConnectionId::new(20))
        .expect("register logical node");

    assert_eq!(node.route_incoming(&actors, actor_id), Ok(route));
    let destination = topology
        .schedulers()
        .find(|scheduler| *scheduler != route.scheduler())
        .expect("migration destination");
    let ticket = actors
        .begin_migration(route, destination)
        .expect("begin actor migration");
    let migrated = actors
        .complete_migration(ticket)
        .expect("complete actor migration");

    assert_eq!(node.route_incoming(&actors, actor_id), Ok(migrated));
    assert_eq!(state.registered_len(), 1);
}

/// Proves orderly shutdown closes admission before registration removal.
#[test]
fn node_shutdown_closes_admission_before_unregistering() {
    let mut state = ServerState::new(ServerOptions::new(4369));
    let mut node = ready_node("shutdown@host", 8083);
    node.register(&mut state, ConnectionId::new(30))
        .expect("register logical node");
    assert!(node.admits_transport());
    assert!(matches!(
        node.unregister(&mut state),
        Err(VmLogicalNodeLifecycleError::InvalidTransition {
            phase: VmLogicalNodePhase::Registered,
            operation: "unregister"
        })
    ));

    node.close_admission().expect("close admission");
    assert!(!node.admits_transport());
    node.unregister(&mut state).expect("unregister node");
    node.acknowledge_stopped().expect("stop node");
    assert_eq!(node.phase(), VmLogicalNodePhase::Stopped);
    assert_eq!(state.registered_len(), 0);
}

/// Proves fail-stop cleanup does not depend on a scheduler owner response.
#[test]
fn fail_stop_unregisters_without_scheduler_cooperation() {
    let mut state = ServerState::new(ServerOptions::new(4369));
    let mut node = ready_node("failed@host", 8084);
    node.register(&mut state, ConnectionId::new(40))
        .expect("register logical node");

    node.fail_stop(&mut state).expect("fail-stop logical node");

    assert_eq!(node.phase(), VmLogicalNodePhase::Stopped);
    assert_eq!(state.registered_len(), 0);
}

/// Proves deterministic client planning retains EPMD-compatible wire behavior.
#[test]
fn client_planning_preserves_otp_compatible_frames_and_output() {
    assert_eq!(
        names_request_frame().expect("names request frame"),
        encode_frame(&[NAMES_REQ]).expect("protocol names frame")
    );
    assert!(stop_request_frame("node@host")
        .expect("stop frame")
        .ends_with(b"node@host\0"));
    assert_eq!(kill_client_output(b"OK").exit_code, 0);
    assert_eq!(kill_client_output(b"NO").exit_code, 1);
    assert_eq!(names_like_client_output(&[0, 0, 17, 17], true).exit_code, 0);
}

/// Proves endpoint construction and STOP parsing fail closed at boundaries.
#[test]
fn endpoint_validation_fails_closed() {
    assert_eq!(
        VmLogicalNodeEndpoint::new(
            Vec::new(),
            NonZeroU16::new(1).expect("port"),
            Vec::new()
        ),
        Err(VmLogicalNodeLifecycleError::InvalidNodeName)
    );
    assert_eq!(
        parse_payload(&[b's', b'n', b'o', b'd', b'e', 0]),
        Ok(Request::Stop(NameRequest {
            name: b"node".to_vec()
        }))
    );
}

/// Proves the scheduler connection handler retains a valid ALIVE2 registration.
#[test]
fn fixed_scheduler_connection_handler_owns_alive_registration() {
    let state = shared_state(ServerOptions::new(4369));
    let mut payload = vec![ALIVE2_REQ];
    payload.extend_from_slice(&8085_u16.to_be_bytes());
    payload.extend_from_slice(&[77, 0]);
    payload.extend_from_slice(&6_u16.to_be_bytes());
    payload.extend_from_slice(&5_u16.to_be_bytes());
    payload.extend_from_slice(&9_u16.to_be_bytes());
    payload.extend_from_slice(b"wire@host");
    payload.extend_from_slice(&0_u16.to_be_bytes());

    let reply = handle_payload(&state, ConnectionId::new(50), &payload)
        .expect("handle alive payload");

    assert!(reply.keep_connection);
    assert_eq!(reply.bytes, vec![b'v', 0, 0, 0, 0, 4]);
    assert!(state
        .lock()
        .expect("registry lock")
        .lookup(b"wire@host")
        .is_some());
}

/// Proves invalid ALIVE2 names receive failure without registry mutation.
#[test]
fn fixed_scheduler_connection_handler_rejects_bad_alive_name_without_registration() {
    let state = shared_state(ServerOptions::new(4369));
    let mut payload = vec![ALIVE2_REQ];
    payload.extend_from_slice(&8086_u16.to_be_bytes());
    payload.extend_from_slice(&[77, 0]);
    payload.extend_from_slice(&6_u16.to_be_bytes());
    payload.extend_from_slice(&5_u16.to_be_bytes());
    payload.extend_from_slice(&0_u16.to_be_bytes());
    payload.extend_from_slice(&0_u16.to_be_bytes());

    let reply = handle_payload(&state, ConnectionId::new(60), &payload)
        .expect("invalid alive payload receives protocol rejection");

    assert!(!reply.keep_connection);
    assert_eq!(reply.bytes, vec![b'v', 1, 0, 0, 0, 0]);
    assert_eq!(state.lock().expect("registry lock").registered_len(), 0);
}

/// Proves node routing publishes only through the actor's current owner entry.
#[test]
fn logical_node_router_publishes_to_current_actor_owner() {
    let topology = VmSchedulerTopology::new(2).expect("topology");
    let actors = Arc::new(VmFixedSchedulerControl::<Vec<u8>>::default());
    let actor_id = NonZeroU64::new(1).expect("actor");
    let original = topology.route(actor_id);
    actors.register(original).expect("register actor");
    let decoder: VmNodePayloadDecoder<Vec<u8>> = Arc::new(|bytes| Ok(bytes.to_vec()));
    let router = VmNodeTransportRouter::new(Arc::clone(&actors), decoder);

    assert!(router.route(actor_id, b"closed").is_err());
    router.open_admission();
    assert_eq!(router.route(actor_id, b"first"), Ok(original));
    let destination = VmSchedulerId::primary()
        .index()
        .checked_add(1)
        .and_then(|index| topology.schedulers().find(|scheduler| scheduler.index() == index))
        .expect("secondary scheduler");
    let migrated = actors
        .complete_migration(
            actors
                .begin_migration(original, destination)
                .expect("begin migration"),
        )
        .expect("complete migration");
    assert_eq!(router.route(actor_id, b"second"), Ok(migrated));

    let lease = actors
        .acquire(migrated, migrated.scheduler())
        .expect("acquire migrated actor");
    assert_eq!(
        actors.drain(&lease).expect("drain actor"),
        vec![b"first".to_vec(), b"second".to_vec()]
    );
    actors
        .release(lease, VmActorLifecycle::Parked)
        .expect("release actor");
    router.close_admission();
    assert!(router.route(actor_id, b"late").is_err());
}

/// Proves logical-node framing is deterministic and bounded before allocation.
#[test]
fn logical_node_transport_frame_is_bounded_and_actor_addressed() {
    let actor = NonZeroU64::new(42).expect("actor");
    let frame = encode_node_transport_frame(actor, b"payload").expect("frame");

    assert_eq!(u32::from_be_bytes(frame[..4].try_into().expect("length")), 15);
    assert_eq!(
        u64::from_be_bytes(frame[4..12].try_into().expect("actor")),
        42
    );
    assert_eq!(&frame[12..], b"payload");
    assert!(encode_node_transport_frame(actor, &vec![0; VM_NODE_MAX_FRAME_BYTES]).is_err());
}

/// Proves production bootstrap discovery, owner migration, and withdrawal.
#[test]
#[ignore = "requires loopback socket access"]
fn logical_node_bootstrap_runs_discovery_transport_and_shutdown_full_cycle() {
    let topology = VmSchedulerTopology::new(2).expect("topology");
    let epmd_listener = bind_protocol_listener("127.0.0.1", 0).expect("epmd listener");
    let epmd_addr = epmd_listener.local_addr().expect("epmd address");
    let state = shared_state(ServerOptions::new(epmd_addr.port()));
    let mut discovery = start_protocol_tasks_with_topology(
        epmd_listener,
        super::transport::protocol_factory(Arc::clone(&state)),
        topology.clone(),
    )
    .expect("start discovery");
    let actors = Arc::new(VmFixedSchedulerControl::<Vec<u8>>::default());
    let actor_id = NonZeroU64::new(1).expect("actor");
    let original = topology.route(actor_id);
    actors.register(original).expect("register actor");
    let listener = bind_protocol_listener("127.0.0.1", 0).expect("node listener");
    let decoder: VmNodePayloadDecoder<Vec<u8>> = Arc::new(|bytes| Ok(bytes.to_vec()));
    let mut node = VmLogicalNodeBootstrap::start(
        listener,
        b"production@local".to_vec(),
        b"terlan".to_vec(),
        topology.clone(),
        Arc::clone(&actors),
        decoder,
        Arc::clone(&state),
    )
    .expect("start logical node");

    assert_eq!(node.phase(), VmLogicalNodePhase::Registered);
    assert!(node.admits_transport());
    assert_eq!(
        query_discovery(epmd_addr, b"production@local"),
        expected_discovery_prefix(node.transport_addr().port())
    );
    assert_eq!(
        send_node_message(node.transport_addr(), actor_id, b"first"),
        vec![0, 0, 0, 0, 0]
    );
    let destination = topology
        .schedulers()
        .find(|scheduler| *scheduler != original.scheduler())
        .expect("migration destination");
    let migrated = actors
        .complete_migration(
            actors
                .begin_migration(original, destination)
                .expect("begin migration"),
        )
        .expect("complete migration");
    assert_eq!(
        send_node_message(node.transport_addr(), actor_id, b"second"),
        vec![0, 0, 0, 0, 1]
    );
    let lease = actors
        .acquire(migrated, migrated.scheduler())
        .expect("acquire actor");
    assert_eq!(
        actors.drain(&lease).expect("drain messages"),
        vec![b"first".to_vec(), b"second".to_vec()]
    );
    actors
        .release(lease, VmActorLifecycle::Parked)
        .expect("release actor");

    node.stop().expect("stop logical node");
    assert_eq!(node.phase(), VmLogicalNodePhase::Stopped);
    assert_eq!(
        query_discovery(epmd_addr, b"production@local"),
        vec![PORT2_RESP, 1]
    );
    discovery.stop().expect("stop discovery");
}

/// Queries one node through the real EPMD socket protocol.
fn query_discovery(address: std::net::SocketAddr, name: &[u8]) -> Vec<u8> {
    let mut payload = vec![PORT2_REQ];
    payload.extend_from_slice(name);
    let request = encode_frame(&payload).expect("lookup frame");
    let mut stream = connect(address);
    stream.write_all(&request).expect("write lookup");
    let mut response = Vec::new();
    stream.read_to_end(&mut response).expect("read lookup");
    response
}

/// Sends one actor-addressed payload and returns its terminal acknowledgement.
fn send_node_message(
    address: std::net::SocketAddr,
    actor: NonZeroU64,
    payload: &[u8],
) -> Vec<u8> {
    let mut stream = connect(address);
    stream
        .write_all(&encode_node_transport_frame(actor, payload).expect("node frame"))
        .expect("write node frame");
    let mut response = Vec::new();
    stream.read_to_end(&mut response).expect("read node ack");
    response
}

/// Opens one bounded loopback test connection.
fn connect(address: std::net::SocketAddr) -> TcpStream {
    let stream = TcpStream::connect_timeout(&address, Duration::from_secs(5))
        .expect("connect loopback service");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    stream
}

/// Returns the stable successful PORT2 prefix for one advertised port.
fn expected_discovery_prefix(port: u16) -> Vec<u8> {
    let mut expected = vec![PORT2_RESP, 0];
    expected.extend_from_slice(&port.to_be_bytes());
    expected.extend_from_slice(&[77, 0, 0, 6, 0, 5, 0, 16]);
    expected.extend_from_slice(b"production@local");
    expected.extend_from_slice(&[0, 6]);
    expected.extend_from_slice(b"terlan");
    expected
}
