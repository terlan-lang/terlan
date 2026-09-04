use std::io::{Cursor, Write};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::*;
use crate::runtime::vm::process::{VmProcessSource, VmProcessState};
use crate::runtime::vm::scheduler::VmSchedulerConfig;
use crate::runtime::vm::{
    execution_shard_epoch::{
        VmShardEpochOperation, VmShardOperationId, VmShardOperationKind, VmShardReplayPolicy,
    },
    execution_shard_protocol::VmShardEpoch,
};
use crate::terlan_native_boundary::capability_wire::{
    read_json_frame, write_json_frame, CapabilityOutcome,
};

/// Thread-safe byte sink used to inspect background writer output.
#[derive(Clone, Default)]
struct CapturedFrames {
    bytes: Arc<Mutex<Vec<u8>>>,
}

impl CapturedFrames {
    /// Returns a stable copy of bytes written before this call.
    fn snapshot(&self) -> Vec<u8> {
        self.bytes.lock().expect("captured frames lock").clone()
    }
}

impl Write for CapturedFrames {
    /// Appends one complete writer chunk to shared test storage.
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes
            .lock()
            .map_err(|_| std::io::Error::other("captured frames lock poisoned"))?
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    /// Flushes the in-memory sink without side effects.
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Creates one process, scheduler, and timer table for client tests.
fn runtime() -> (VmProcessTable, VmScheduler, VmTimerTable, VmProcessId) {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(VmProcessSource::new("app.Native", "call", 0));
    (
        processes,
        VmScheduler::new(VmSchedulerConfig::new(10, 100)),
        VmTimerTable::default(),
        owner,
    )
}

/// Creates an exact test worker generation.
fn worker_identity() -> VmCapabilityWorkerIdentity {
    worker_identity_for(1)
}

/// Creates one selected generation of the test worker slot.
fn worker_identity_for(generation: u64) -> VmCapabilityWorkerIdentity {
    VmCapabilityWorkerIdentity::new(
        VmCapabilityWorkerId::new("adapter-primary").expect("worker id"),
        VmCapabilityWorkerGeneration::new(generation).expect("worker generation"),
    )
}

/// Creates explicit capability and shard-epoch ownership for one request.
fn request_context(capability: &str) -> VmCapabilityRequestContext {
    VmCapabilityRequestContext::new(
        VmCapabilityId::new(capability).expect("capability id"),
        VmShardEpochOperation::new(
            VmShardOperationId::new(1).expect("operation id"),
            VmShardEpoch::new(1).expect("shard epoch"),
            VmShardOperationKind::CapabilityCompletion,
            VmShardReplayPolicy::AtMostOnce,
        ),
    )
    .expect("capability request context")
}

/// Builds an attached client around deterministic in-memory streams.
fn client_with_responses(
    responses: Vec<CapabilityResponse>,
) -> (VmCapabilityWorkerClient, CapturedFrames) {
    let mut output = Vec::new();
    for response in responses {
        write_json_frame(&mut output, &response, 4_096).expect("response fixture");
    }
    client_with_output(output)
}

/// Builds an attached client around raw deterministic worker output.
fn client_with_output(output: Vec<u8>) -> (VmCapabilityWorkerClient, CapturedFrames) {
    client_with_output_for_generation(output, 1)
}

/// Builds a deterministic client for one exact worker generation.
fn client_with_output_for_generation(
    output: Vec<u8>,
    generation: u64,
) -> (VmCapabilityWorkerClient, CapturedFrames) {
    let captured = CapturedFrames::default();
    let transport = VmCapabilityWorkerTransport::from_streams(
        captured.clone(),
        Cursor::new(output),
        4_096,
        1,
        None,
        None,
    )
    .expect("attached transport");
    (
        VmCapabilityWorkerClient {
            identity: worker_identity_for(generation),
            transport,
            deadlines: VmNativeBoundaryDeadlineQueue::new(1),
            pending_contexts: BTreeMap::new(),
            parked_contexts: BTreeMap::new(),
            late_cleanup: parked::VmCapabilityLateCleanupState::default(),
            capabilities: BTreeSet::from(["example".to_string(), "postgres".to_string()]),
            last_request_id: RequestId { value: 0 },
            remote_credit_limit: 1,
        },
        captured,
    )
}

/// Rejects empty identities, zero generations, and non-capability contexts.
#[test]
fn capability_worker_request_identity_is_closed_and_typed() {
    assert!(VmCapabilityWorkerId::new(" ").is_err());
    assert!(VmCapabilityId::new("").is_err());
    assert!(VmCapabilityWorkerGeneration::new(0).is_err());
    assert_eq!(worker_identity().id.as_str(), "adapter-primary");
    assert_eq!(worker_identity().generation.as_u64(), 1);
    let wrong_kind = VmShardEpochOperation::new(
        VmShardOperationId::new(9).expect("operation id"),
        VmShardEpoch::new(1).expect("shard epoch"),
        VmShardOperationKind::ActorRoute,
        VmShardReplayPolicy::AtMostOnce,
    );
    assert!(VmCapabilityRequestContext::new(
        VmCapabilityId::new("postgres").expect("capability"),
        wrong_kind,
    )
    .is_err());
}

/// Undeclared capability use fails before actor parking or transport mutation.
#[test]
fn capability_worker_rejects_undeclared_capability_before_parking() {
    let (mut client, captured) = client_with_responses(Vec::new());
    let (mut processes, mut scheduler, mut timers, owner) = runtime();
    let error = client
        .start_call(
            &mut VmCapabilityWorkerRuntime {
                timers: &mut timers,
                processes: &mut processes,
                scheduler: &mut scheduler,
            },
            crate::runtime::vm::capability_worker::VmCapabilityWorkerCall {
                owner: owner,
                context: request_context("filesystem"),
                operation: ("std.fs.read").into(),
                arguments: Vec::new(),
                now_tick: 0,
                timeout_ticks: 10,
            },
        )
        .expect_err("undeclared capability");

    assert!(error.contains("capability `filesystem` is not granted"));
    assert_eq!(client.pending_len(), 0);
    assert_eq!(captured.snapshot(), Vec::<u8>::new());
    assert_eq!(
        processes.get(owner).expect("owner").state,
        VmProcessState::Runnable
    );
}

/// Reused request numbers remain isolated by exact worker process generation.
#[test]
fn capability_worker_restart_generation_attributes_reused_request_ids() {
    let response = CapabilityResponse::Reply {
        version: CAPABILITY_PROTOCOL_VERSION,
        request_id: 1,
        reserved_credits: 0,
        available_credits: 1,
        outcome: CapabilityOutcome::Ok {
            value: CapabilityValue::Unit,
        },
    };
    for generation in [1, 2] {
        let mut output = Vec::new();
        write_json_frame(&mut output, &response, 4_096).expect("response fixture");
        let (mut client, _) = client_with_output_for_generation(output, generation);
        let (mut processes, mut scheduler, mut timers, owner) = runtime();
        client
            .start_call(
                &mut VmCapabilityWorkerRuntime {
                    timers: &mut timers,
                    processes: &mut processes,
                    scheduler: &mut scheduler,
                },
                crate::runtime::vm::capability_worker::VmCapabilityWorkerCall {
                    owner: owner,
                    context: request_context("example"),
                    operation: ("std.example.call").into(),
                    arguments: Vec::new(),
                    now_tick: 0,
                    timeout_ticks: 10,
                },
            )
            .expect("park generation request");
        let completion =
            wait_for_completion(&mut client, &mut timers, &mut processes, &mut scheduler);
        let VmCapabilityWorkerCompletion::Reply {
            worker, request_id, ..
        } = completion
        else {
            panic!("expected attributed reply");
        };
        assert_eq!(worker, worker_identity_for(generation));
        assert_eq!(request_id, RequestId { value: 1 });
    }
}

/// Dropping a saturated bounded event queue cannot strand its reader thread.
#[test]
fn capability_worker_response_queue_is_bounded_and_drop_safe() {
    let mut output = Vec::new();
    for _ in 0..8 {
        write_json_frame(
            &mut output,
            &CapabilityResponse::ShutdownAck {
                version: CAPABILITY_PROTOCOL_VERSION,
            },
            4_096,
        )
        .expect("response fixture");
    }
    let (client, _) = client_with_output_for_generation(output, 1);
    drop(client);
}

/// Waits until the background writer has emitted the expected frame count.
fn wait_for_frames(captured: &CapturedFrames, expected: usize) -> Vec<CapabilityRequest> {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let bytes = captured.snapshot();
        let count = bytes.iter().filter(|byte| **byte == b'\n').count();
        if count >= expected {
            let mut input = Cursor::new(bytes);
            let mut requests = Vec::new();
            while let Some(request) =
                read_json_frame(&mut input, 4_096).expect("captured request frame")
            {
                requests.push(request);
            }
            return requests;
        }
        assert!(
            Instant::now() < deadline,
            "background writer did not emit {expected} frames"
        );
        std::thread::yield_now();
    }
}

/// Polls until one asynchronous worker event reaches the VM.
fn wait_for_completion(
    client: &mut VmCapabilityWorkerClient,
    timers: &mut VmTimerTable,
    processes: &mut VmProcessTable,
    scheduler: &mut VmScheduler,
) -> VmCapabilityWorkerCompletion {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if let Some(completion) = client
            .poll(&mut VmCapabilityWorkerRuntime {
                timers,
                processes,
                scheduler,
            })
            .expect("poll worker event")
        {
            return completion;
        }
        assert!(
            Instant::now() < deadline,
            "worker response did not reach the VM"
        );
        std::thread::yield_now();
    }
}

/// Completes a parked call through request correlation and one scheduler wakeup.
#[test]
fn capability_worker_reply_completes_live_vm_deadline() {
    let response = CapabilityResponse::Reply {
        version: CAPABILITY_PROTOCOL_VERSION,
        request_id: 1,
        reserved_credits: 0,
        available_credits: 1,
        outcome: CapabilityOutcome::Ok {
            value: CapabilityValue::Bool(true),
        },
    };
    let (mut client, captured) = client_with_responses(vec![response]);
    let (mut processes, mut scheduler, mut timers, owner) = runtime();
    let context = request_context("example");
    let scheduled = client
        .start_call(
            &mut VmCapabilityWorkerRuntime {
                timers: &mut timers,
                processes: &mut processes,
                scheduler: &mut scheduler,
            },
            crate::runtime::vm::capability_worker::VmCapabilityWorkerCall {
                owner: owner,
                context: context.clone(),
                operation: ("std.example.call").into(),
                arguments: vec![NativeBoundaryTerm::Text("input".to_string())],
                now_tick: 4,
                timeout_ticks: 10,
            },
        )
        .expect("park external call");

    assert_eq!(scheduled.request_id, RequestId { value: 1 });
    assert_eq!(
        processes.get(owner).expect("owner").state,
        VmProcessState::Blocked
    );
    let completion = wait_for_completion(&mut client, &mut timers, &mut processes, &mut scheduler);
    assert_eq!(
        completion,
        VmCapabilityWorkerCompletion::Reply {
            worker: worker_identity(),
            request_id: RequestId { value: 1 },
            context,
            reply: NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Bool(true)),
        }
    );
    assert_eq!(
        processes.get(owner).expect("owner").state,
        VmProcessState::Runnable
    );
    assert_eq!(client.pending_len(), 0);

    let requests = wait_for_frames(&captured, 1);
    assert!(matches!(
        &requests[0],
        CapabilityRequest::Call {
            request_id: 1,
            owner_id,
            capability,
            operation,
            ..
        } if *owner_id == owner.as_u64()
            && capability == "example"
            && operation == "std.example.call"
    ));
}

/// Suppresses a reply after cancellation and emits a cancellation frame.
#[test]
fn capability_worker_cancellation_wins_over_late_reply() {
    let response = CapabilityResponse::Reply {
        version: CAPABILITY_PROTOCOL_VERSION,
        request_id: 1,
        reserved_credits: 0,
        available_credits: 1,
        outcome: CapabilityOutcome::Ok {
            value: CapabilityValue::Unit,
        },
    };
    let (mut client, captured) = client_with_responses(vec![response]);
    let (mut processes, mut scheduler, mut timers, owner) = runtime();
    let scheduled = client
        .start_call(
            &mut VmCapabilityWorkerRuntime {
                timers: &mut timers,
                processes: &mut processes,
                scheduler: &mut scheduler,
            },
            crate::runtime::vm::capability_worker::VmCapabilityWorkerCall {
                owner: owner,
                context: request_context("example"),
                operation: ("std.example.call").into(),
                arguments: Vec::new(),
                now_tick: 0,
                timeout_ticks: 10,
            },
        )
        .expect("park call");
    let terminal = client
        .cancel(
            &mut VmCapabilityWorkerRuntime {
                timers: &mut timers,
                processes: &mut processes,
                scheduler: &mut scheduler,
            },
            scheduled.timer_id,
        )
        .expect("cancel call");

    assert!(matches!(
        terminal.completion,
        VmNativeBoundaryDeadlineCompletion::Cancelled { .. }
    ));
    assert_eq!(terminal.cancellation_error, None);
    assert_eq!(
        wait_for_completion(&mut client, &mut timers, &mut processes, &mut scheduler,),
        VmCapabilityWorkerCompletion::StaleReply {
            worker: worker_identity(),
            request_id: RequestId { value: 1 }
        }
    );
    assert_eq!(
        processes.get(owner).expect("owner").state,
        VmProcessState::Runnable
    );

    let requests = wait_for_frames(&captured, 2);
    assert!(matches!(
        requests[1],
        CapabilityRequest::Cancel {
            request_id: 1,
            owner_id,
            ..
        } if owner_id == owner.as_u64()
    ));
}

/// Converts an expired VM deadline into timeout and worker cancellation.
#[test]
fn capability_worker_timeout_wakes_owner_and_delivers_cancellation() {
    let (mut client, captured) = client_with_responses(Vec::new());
    let (mut processes, mut scheduler, mut timers, owner) = runtime();
    client
        .start_call(
            &mut VmCapabilityWorkerRuntime {
                timers: &mut timers,
                processes: &mut processes,
                scheduler: &mut scheduler,
            },
            crate::runtime::vm::capability_worker::VmCapabilityWorkerCall {
                owner: owner,
                context: request_context("example"),
                operation: ("std.example.slow").into(),
                arguments: Vec::new(),
                now_tick: 0,
                timeout_ticks: 2,
            },
        )
        .expect("park slow call");
    let events = timers.advance_clock(&mut processes, &mut scheduler, 2);
    let terminal = client
        .handle_timer_event(
            &mut VmCapabilityWorkerRuntime {
                timers: &mut timers,
                processes: &mut processes,
                scheduler: &mut scheduler,
            },
            &events[0],
        )
        .expect("handle deadline")
        .expect("owned deadline");

    assert!(matches!(
        terminal.completion,
        VmNativeBoundaryDeadlineCompletion::TimedOut { .. }
    ));
    assert_eq!(terminal.cancellation_error, None);
    assert_eq!(
        processes.get(owner).expect("owner").state,
        VmProcessState::Runnable
    );
    assert_eq!(client.pending_len(), 0);
    let requests = wait_for_frames(&captured, 2);
    assert!(matches!(requests[1], CapabilityRequest::Cancel { .. }));
}

/// Wakes every parked owner immediately when the worker response stream closes.
#[test]
fn capability_worker_eof_cancels_pending_vm_requests() {
    let (mut client, _captured) = client_with_responses(Vec::new());
    let (mut processes, mut scheduler, mut timers, owner) = runtime();
    client
        .start_call(
            &mut VmCapabilityWorkerRuntime {
                timers: &mut timers,
                processes: &mut processes,
                scheduler: &mut scheduler,
            },
            crate::runtime::vm::capability_worker::VmCapabilityWorkerCall {
                owner: owner,
                context: request_context("example"),
                operation: ("std.example.call").into(),
                arguments: Vec::new(),
                now_tick: 0,
                timeout_ticks: 100,
            },
        )
        .expect("park call before EOF");

    let completion = wait_for_completion(&mut client, &mut timers, &mut processes, &mut scheduler);
    let VmCapabilityWorkerCompletion::TransportClosed { worker, cancelled } = completion else {
        panic!("expected closed transport");
    };
    assert_eq!(worker, worker_identity());
    assert_eq!(cancelled.len(), 1);
    assert!(matches!(
        cancelled[0],
        VmNativeBoundaryDeadlineCompletion::Cancelled { .. }
    ));
    assert_eq!(
        processes.get(owner).expect("owner").state,
        VmProcessState::Runnable
    );
    assert_eq!(client.pending_len(), 0);
}

/// Quarantines a worker that reports impossible credit state and wakes callers.
#[test]
fn capability_worker_protocol_failure_closes_transport_and_cancels_pending() {
    let response = CapabilityResponse::Reply {
        version: CAPABILITY_PROTOCOL_VERSION,
        request_id: 1,
        reserved_credits: 0,
        available_credits: 2,
        outcome: CapabilityOutcome::Ok {
            value: CapabilityValue::Unit,
        },
    };
    let (mut client, _captured) = client_with_responses(vec![response]);
    let (mut processes, mut scheduler, mut timers, owner) = runtime();
    client
        .start_call(
            &mut VmCapabilityWorkerRuntime {
                timers: &mut timers,
                processes: &mut processes,
                scheduler: &mut scheduler,
            },
            crate::runtime::vm::capability_worker::VmCapabilityWorkerCall {
                owner: owner,
                context: request_context("example"),
                operation: ("std.example.call").into(),
                arguments: Vec::new(),
                now_tick: 0,
                timeout_ticks: 100,
            },
        )
        .expect("park call before malformed reply");

    let completion = wait_for_completion(&mut client, &mut timers, &mut processes, &mut scheduler);
    let VmCapabilityWorkerCompletion::TransportFailed {
        worker,
        error,
        cancelled,
    } = completion
    else {
        panic!("expected failed transport");
    };
    assert_eq!(worker, worker_identity());
    assert!(error.contains("capability_worker.credit"));
    assert_eq!(cancelled.len(), 1);
    assert_eq!(
        processes.get(owner).expect("owner").state,
        VmProcessState::Runnable
    );
    assert!(client.shutdown().is_err());
}

/// Rejects relative executables and zero policy bounds before process creation.
#[test]
fn capability_worker_policy_is_closed_and_bounded() {
    assert!(VmCapabilityWorkerPolicy::new(
        "relative-worker",
        NativeBoundaryExecutionProfile::ExternalAdapter,
    )
    .is_err());
    let policy = VmCapabilityWorkerPolicy::new(
        "/tmp/terlan-native-worker",
        NativeBoundaryExecutionProfile::CrashIsolated,
    )
    .expect("absolute worker")
    .allow("postgres")
    .admit_worker_class("blocking")
    .with_max_payload_bytes(4_096)
    .expect("payload limit")
    .with_max_requests(8)
    .expect("request limit")
    .with_credit_limit(2)
    .expect("credit limit");

    assert_eq!(policy.capabilities, vec!["postgres"]);
    assert_eq!(policy.worker_classes, vec!["blocking"]);
    assert_eq!(
        policy.execution_profile,
        NativeBoundaryExecutionProfile::CrashIsolated
    );
    assert!(policy.clone().with_max_payload_bytes(0).is_err());
    assert!(policy.clone().with_max_requests(0).is_err());
    assert!(policy.with_credit_limit(0).is_err());
}

/// Runs one real child process from policy admission through orderly shutdown.
#[test]
#[ignore = "requires a separately built terlan-native-worker executable"]
fn capability_worker_process_transport_runs_full_cycle() {
    let executable = std::env::var_os("TERLAN_TEST_CAPABILITY_WORKER")
        .expect("TERLAN_TEST_CAPABILITY_WORKER must name the built worker");
    let policy = VmCapabilityWorkerPolicy::new(
        PathBuf::from(executable),
        NativeBoundaryExecutionProfile::ExternalAdapter,
    )
    .expect("absolute worker path")
    .allow("postgres")
    .admit_worker_class("fast")
    .with_max_payload_bytes(4_096)
    .expect("payload limit")
    .with_max_requests(4)
    .expect("request limit")
    .with_credit_limit(1)
    .expect("credit limit");
    let mut client =
        VmCapabilityWorkerClient::spawn(worker_identity(), policy).expect("spawn worker");
    let (mut processes, mut scheduler, mut timers, owner) = runtime();
    client
        .start_call(
            &mut VmCapabilityWorkerRuntime {
                timers: &mut timers,
                processes: &mut processes,
                scheduler: &mut scheduler,
            },
            crate::runtime::vm::capability_worker::VmCapabilityWorkerCall {
                owner: owner,
                context: request_context("postgres"),
                operation: ("std.db.postgres.string").into(),
                arguments: vec![
                    NativeBoundaryTerm::Handle {
                        id: 1,
                        generation: 1,
                    },
                    NativeBoundaryTerm::Text("name".to_string()),
                ],
                now_tick: 0,
                timeout_ticks: 100,
            },
        )
        .expect("park real worker call");

    let completion = wait_for_completion(&mut client, &mut timers, &mut processes, &mut scheduler);
    assert!(
        matches!(
        &completion,
        VmCapabilityWorkerCompletion::Reply {
            reply: NativeBoundaryReplyTerm::Error { code, .. },
            ..
        } if code == "resource.stale_handle"
        ),
        "unexpected real-worker completion: {completion:?}"
    );
    assert_eq!(
        processes.get(owner).expect("owner").state,
        VmProcessState::Runnable
    );

    client.shutdown().expect("queue shutdown");
    assert_eq!(
        wait_for_completion(&mut client, &mut timers, &mut processes, &mut scheduler,),
        VmCapabilityWorkerCompletion::ShutdownAcknowledged {
            worker: worker_identity()
        }
    );
}

/// Proves the production sandbox closes a deliberately inherited host descriptor.
#[test]
#[ignore = "requires a separately built terlan-native-worker executable"]
fn capability_worker_sandbox_closes_inherited_descriptor() {
    let executable = std::env::var_os("TERLAN_TEST_CAPABILITY_WORKER")
        .expect("TERLAN_TEST_CAPABILITY_WORKER must name the built worker");
    let profile = CapabilitySandboxProfile::current().expect("host sandbox profile");
    let capabilities = vec!["postgres".to_string()];
    let sandbox_dir =
        sandbox::VmCapabilityWorkerSandboxDir::create().expect("private sandbox directory");
    let sandbox_command = sandbox::worker_command(
        profile,
        &PathBuf::from(executable),
        &capabilities,
        sandbox_dir.path(),
    )
    .expect("sandbox command");
    let program = sandbox_command.get_program().to_os_string();
    let arguments = sandbox_command
        .get_args()
        .map(|argument| argument.to_os_string())
        .collect::<Vec<_>>();

    let mut command = Command::new("/bin/bash");
    command
        .arg("-c")
        .arg("exec 142</dev/null; exec \"$@\"")
        .arg("terlan-capability-worker-fd-test")
        .arg(program)
        .args(arguments)
        .arg("--allow")
        .arg("postgres")
        .arg("--worker-class")
        .arg("fast")
        .arg("--execution-profile")
        .arg(NativeBoundaryExecutionProfile::ExternalAdapter.protocol_name())
        .arg("--sandbox-profile")
        .arg(profile.name())
        .arg("--max-payload-bytes")
        .arg("4096")
        .arg("--max-requests")
        .arg("4")
        .arg("--credit-limit")
        .arg("1")
        .env_clear()
        .current_dir(sandbox_dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn().expect("spawn descriptor test wrapper");
    let input = child.stdin.take().expect("worker stdin");
    let output = child.stdout.take().expect("worker stdout");
    let transport = VmCapabilityWorkerTransport::from_streams(
        input,
        output,
        4_096,
        1,
        Some(child),
        Some(sandbox_dir),
    )
    .expect("attached sandbox transport");
    let mut client = VmCapabilityWorkerClient {
        identity: worker_identity(),
        transport,
        deadlines: VmNativeBoundaryDeadlineQueue::new(1),
        pending_contexts: BTreeMap::new(),
        parked_contexts: BTreeMap::new(),
        late_cleanup: parked::VmCapabilityLateCleanupState::default(),
        capabilities: BTreeSet::from(["postgres".to_string()]),
        last_request_id: RequestId { value: 0 },
        remote_credit_limit: 1,
    };
    let (mut processes, mut scheduler, mut timers, owner) = runtime();
    client
        .start_call(
            &mut VmCapabilityWorkerRuntime {
                timers: &mut timers,
                processes: &mut processes,
                scheduler: &mut scheduler,
            },
            crate::runtime::vm::capability_worker::VmCapabilityWorkerCall {
                owner: owner,
                context: request_context("postgres"),
                operation: ("std.db.postgres.string").into(),
                arguments: vec![
                    NativeBoundaryTerm::Handle {
                        id: 1,
                        generation: 1,
                    },
                    NativeBoundaryTerm::Text("name".to_string()),
                ],
                now_tick: 0,
                timeout_ticks: 100,
            },
        )
        .expect("park descriptor-test call");

    let completion = wait_for_completion(&mut client, &mut timers, &mut processes, &mut scheduler);
    assert!(
        matches!(
            &completion,
            VmCapabilityWorkerCompletion::Reply {
                reply: NativeBoundaryReplyTerm::Error { code, .. },
                ..
            } if code == "resource.stale_handle"
        ),
        "unexpected descriptor-test completion: {completion:?}"
    );
    client.shutdown().expect("queue shutdown");
    assert_eq!(
        wait_for_completion(&mut client, &mut timers, &mut processes, &mut scheduler),
        VmCapabilityWorkerCompletion::ShutdownAcknowledged {
            worker: worker_identity()
        }
    );
}
