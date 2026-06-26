use std::collections::BTreeMap;
use std::io::{self, ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use terlan_hir::{syntax_module_output_to_interface, ModuleInterface};
use terlan_syntax::{parse_interface_module_as_syntax_output, span::Span, SyntaxModuleOutput};
use terlan_typeck::{
    CoreEffectSet, CoreExpr, CoreIntrinsicCall, CoreIntrinsicId, CoreModule, CoreModuleMetadata,
    CorePrimitiveIntrinsic, CoreRuntimeCapability, CoreSourceIdentity, CoreType, CORE_IR_SCHEMA,
};

/// Builds a minimal syntax-aware CoreIR module for backend gate tests.
///
/// Inputs:
/// - `module`: parsed syntax-output fixture.
///
/// Output:
/// - CoreIR module with matching schema, module name, source identity, and
///   interface payload.
///
/// Transformation:
/// - Copies syntax-output identity into CoreIR and derives the public
///   interface through the existing HIR adapter; declaration vectors are
///   left empty because these tests exercise backend identity gating only.
pub(super) fn test_core_module_for_syntax(module: &SyntaxModuleOutput) -> CoreModule {
    CoreModule {
        schema: CORE_IR_SCHEMA.to_string(),
        module: module.module_name.clone(),
        source: CoreSourceIdentity {
            source_kind: format!("{:?}", module.source_kind),
            syntax_contract_fingerprint: Some(module.syntax_contract.fingerprint.clone()),
        },
        imports: Vec::new(),
        exports: Vec::new(),
        types: Vec::new(),
        functions: Vec::new(),
        constructors: Vec::new(),
        trait_conformances: Vec::new(),
        metadata: CoreModuleMetadata {
            interface_function_count: 0,
            interface_type_count: 0,
            constructor_count: 0,
            proof_readiness: terlan_typeck::CoreProofReadiness::NoExpressions,
            lean_covered_expr_count: 0,
            partial_expr_count: 0,
            proof_model_required_expr_count: 0,
            runtime_boundary_expr_count: 0,
            artifact_only_expr_count: 0,
            lean_covered_pattern_count: 0,
            partial_pattern_count: 0,
            proof_model_required_pattern_count: 0,
            runtime_boundary_pattern_count: 0,
            artifact_only_pattern_count: 0,
            typed_core_expr_count: 0,
            summary_only_expr_count: 0,
            typed_core_pattern_count: 0,
            summary_only_pattern_count: 0,
            typed_core_type_count: 0,
            summary_only_type_count: 0,
            checked_preservation_expr_count: 0,
            checked_preservation_pattern_count: 0,
            checked_preservation_expr_structural_count: 0,
            checked_preservation_pattern_structural_count: 0,
            checked_preservation_expr_no_runtime_bindings_count: 0,
            checked_preservation_pattern_no_runtime_bindings_count: 0,
            checked_preservation_expr_runtime_bindings_required_count: 0,
            checked_preservation_pattern_runtime_bindings_required_count: 0,
            resolved_constructor_call_identity_count: 0,
            resolved_constructor_chain_identity_count: 0,
            resolved_constructor_pattern_identity_count: 0,
            unresolved_constructor_call_candidate_count: 0,
            unresolved_constructor_chain_candidate_count: 0,
            unresolved_constructor_pattern_candidate_count: 0,
        },
        interface: syntax_module_output_to_interface(module),
    }
}

/// Adds a parsed interface summary to a test interface map.
///
/// Inputs:
/// - `interfaces`: mutable module-interface map used by syntax bridge tests.
/// - `name`: fully qualified provider module name.
/// - `source`: generated `.typi` summary text.
///
/// Output:
/// - The interface map contains the parsed summary under `name`.
///
/// Transformation:
/// - Parses a generated summary and adapts it through the HIR interface shape
///   expected by Erlang syntax bridge tests.
pub(super) fn add_interface_summary(
    interfaces: &mut BTreeMap<String, ModuleInterface>,
    name: &str,
    source: &str,
) {
    let module = parse_interface_module_as_syntax_output(source).expect("parse interface summary");
    interfaces.insert(name.to_string(), syntax_module_output_to_interface(&module));
}

/// Builds a pure CoreIR string intrinsic call for Erlang backend tests.
///
/// Inputs:
/// - `intrinsic`: string primitive intrinsic identity under test.
/// - `args`: CoreIR argument expressions supplied to the intrinsic.
/// - `return_type`: typed CoreIR result contract for the intrinsic.
///
/// Output:
/// - CoreIR intrinsic call with a pure effect set and empty source span.
///
/// Transformation:
/// - Wraps the primitive identity and arguments in the production CoreIR
///   intrinsic-call shape used by source lowering.
pub(super) fn test_string_intrinsic_call(
    intrinsic: CorePrimitiveIntrinsic,
    args: Vec<CoreExpr>,
    return_type: CoreType,
) -> CoreIntrinsicCall {
    test_primitive_intrinsic_call(intrinsic, args, return_type)
}

/// Builds a pure CoreIR primitive intrinsic call for Erlang backend tests.
///
/// Inputs:
/// - `intrinsic`: primitive intrinsic identity under test.
/// - `args`: CoreIR argument expressions supplied to the intrinsic.
/// - `return_type`: typed CoreIR result contract for the intrinsic.
///
/// Output:
/// - CoreIR intrinsic call with a pure effect set and empty source span.
///
/// Transformation:
/// - Wraps the primitive identity and arguments in the production CoreIR
///   intrinsic-call shape used by source lowering.
pub(super) fn test_primitive_intrinsic_call(
    intrinsic: CorePrimitiveIntrinsic,
    args: Vec<CoreExpr>,
    return_type: CoreType,
) -> CoreIntrinsicCall {
    CoreIntrinsicCall {
        id: CoreIntrinsicId::Primitive(intrinsic),
        args,
        return_type,
        effects: CoreEffectSet {
            effects: vec!["pure".to_string()],
        },
        span: Span::new(0, 0),
    }
}

/// Builds an effectful CoreIR runtime capability call for Erlang backend tests.
///
/// Inputs:
/// - `capability`: runtime capability identity under test.
/// - `args`: CoreIR argument expressions supplied to the capability.
/// - `return_type`: typed CoreIR result contract for the capability.
///
/// Output:
/// - CoreIR intrinsic call with an `io` effect set and empty source span.
///
/// Transformation:
/// - Wraps the runtime capability identity and arguments in the production
///   CoreIR intrinsic-call shape used by source lowering.
pub(super) fn test_runtime_capability_call(
    capability: CoreRuntimeCapability,
    args: Vec<CoreExpr>,
    return_type: CoreType,
) -> CoreIntrinsicCall {
    CoreIntrinsicCall {
        id: CoreIntrinsicId::Runtime(capability),
        args,
        return_type,
        effects: CoreEffectSet {
            effects: vec!["io".to_string()],
        },
        span: Span::new(0, 0),
    }
}

/// Owns a local TCP peer for BEAM primitive integration tests.
///
/// Inputs:
/// - A localhost listener bound to an OS-assigned available port.
/// - A worker thread that accepts loopback clients and handles each connection.
///
/// Output:
/// - A fixture exposing its socket address while guaranteeing shutdown in
///   `Drop`.
///
/// Transformation:
/// - Converts ad hoc per-test socket setup into a reusable peer that waits for
///   readiness with a timeout and cleans up even when assertions fail.
pub(super) struct LocalTcpPeer {
    addr: SocketAddr,
    shutdown: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl LocalTcpPeer {
    /// Starts a local byte-echo peer and waits until it is accepting clients.
    ///
    /// Inputs:
    /// - `readiness_timeout`: maximum time to wait for the peer thread to
    ///   report that it has entered the accept loop.
    ///
    /// Output:
    /// - A reusable local TCP peer fixture.
    ///
    /// Transformation:
    /// - Binds `127.0.0.1:0`, transfers the listener to a worker thread, and
    ///   confirms readiness before returning the assigned address to callers.
    pub(super) fn start_echo(readiness_timeout: Duration) -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutdown);
        let (ready_tx, ready_rx) = mpsc::channel();

        let worker = thread::spawn(move || {
            let _ = ready_tx.send(());
            run_echo_accept_loop(listener, worker_shutdown);
        });

        ready_rx.recv_timeout(readiness_timeout).map_err(|error| {
            shutdown.store(true, Ordering::SeqCst);
            io::Error::new(
                ErrorKind::TimedOut,
                format!("local TCP peer did not become ready: {error}"),
            )
        })?;

        Ok(Self {
            addr,
            shutdown,
            worker: Some(worker),
        })
    }

    /// Returns the socket address assigned to the local TCP peer.
    pub(super) fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Returns the TCP port assigned to the local TCP peer.
    pub(super) fn port(&self) -> u16 {
        self.addr.port()
    }
}

impl Drop for LocalTcpPeer {
    /// Stops the worker thread and releases the local listener.
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_millis(100));
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Runs the nonblocking accept loop for a local TCP echo peer.
fn run_echo_accept_loop(listener: TcpListener, shutdown: Arc<AtomicBool>) {
    while !shutdown.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((mut stream, _peer_addr)) => echo_stream_until_closed(&mut stream, &shutdown),
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(_error) => break,
        }
    }
}

/// Echoes all bytes received on one connection until the client closes it.
fn echo_stream_until_closed(stream: &mut TcpStream, shutdown: &AtomicBool) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(50)));
    let mut buffer = [0_u8; 1024];

    while !shutdown.load(Ordering::SeqCst) {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                if stream.write_all(&buffer[..count]).is_err() {
                    break;
                }
            }
            Err(error)
                if error.kind() == ErrorKind::WouldBlock || error.kind() == ErrorKind::TimedOut => {
            }
            Err(_error) => break,
        }
    }
}

/// Verifies the reusable local TCP peer allocates a port, accepts clients, and
/// echoes bytes before cleanup.
#[test]
fn local_tcp_peer_echoes_bytes_on_allocated_loopback_port() {
    let peer = match LocalTcpPeer::start_echo(Duration::from_secs(2)) {
        Ok(peer) => peer,
        Err(error) if error.kind() == ErrorKind::PermissionDenied => {
            eprintln!("skipping local TCP peer test because loopback bind is denied: {error}");
            return;
        }
        Err(error) => panic!("start local TCP peer: {error}"),
    };

    assert_eq!(peer.addr().ip().to_string(), "127.0.0.1");
    assert!(peer.port() > 0);

    let mut stream =
        TcpStream::connect_timeout(&peer.addr(), Duration::from_secs(2)).expect("connect to peer");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("set read timeout");

    stream.write_all(&[1, 2, 3, 255]).expect("write frame");
    let mut echoed = [0_u8; 4];
    stream.read_exact(&mut echoed).expect("read echoed frame");

    assert_eq!(echoed, [1, 2, 3, 255]);
}
