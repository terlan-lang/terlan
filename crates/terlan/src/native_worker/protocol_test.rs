//! Capability-worker framing and admission tests.

use std::ffi::OsString;
use std::io::Cursor;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::execution::{run_with_executor, CapabilityCall, CapabilityDispose, CapabilityExecutor};
use super::{run_capability_worker, CapabilityWorkerConfig};
use crate::terlan_native_boundary::cancellation::NativeBoundaryCancellationToken;
use crate::terlan_native_boundary::capability_sandbox::LINUX_BWRAP_PROFILE;
use crate::terlan_native_boundary::capability_wire::{
    read_json_frame, write_json_frame, CapabilityOutcome, CapabilityRequest, CapabilityResponse,
    CapabilityValue, CAPABILITY_PROTOCOL_VERSION,
};
use crate::terlan_native_boundary::error::{error_for, ErrorKind};
use crate::terlan_native_boundary::metadata::NativeBoundaryExecutionProfile;
use crate::terlan_native_boundary::request::RequestId;
use crate::terlan_native_boundary::term::{NativeBoundaryReplyTerm, NativeBoundaryTerm};
use crate::terlan_native_boundary::worker::NativeBoundaryWorkerReply;

/// Parses a closed startup policy and rejects an undeclared positional image.
#[test]
fn config_parses_explicit_bounds_and_authority() {
    let config = CapabilityWorkerConfig::parse(&[
        OsString::from("--execution-profile"),
        OsString::from("external-adapter"),
        OsString::from("--sandbox-profile"),
        OsString::from(LINUX_BWRAP_PROFILE),
        OsString::from("--allow"),
        OsString::from("postgres"),
        OsString::from("--worker-class"),
        OsString::from("resource-owning"),
        OsString::from("--max-payload-bytes"),
        OsString::from("4096"),
        OsString::from("--max-requests"),
        OsString::from("7"),
        OsString::from("--credit-limit"),
        OsString::from("2"),
    ])
    .expect("closed worker policy");

    assert!(config.capabilities.contains("postgres"));
    assert_eq!(
        config.execution_profile,
        NativeBoundaryExecutionProfile::ExternalAdapter
    );
    assert!(config.worker_classes.contains("resource-owning"));
    assert_eq!(config.max_payload_bytes, 4096);
    assert_eq!(config.max_requests, 7);
    assert_eq!(config.credit_limit, 2);
    assert!(CapabilityWorkerConfig::parse(&[OsString::from("application.tvm")]).is_err());
    assert!(CapabilityWorkerConfig::parse(&[
        OsString::from("--sandbox-profile"),
        OsString::from("unconfined"),
    ])
    .is_err());
}

/// Requires a declared worker-only execution profile and rejects local work.
#[test]
fn config_requires_a_closed_worker_execution_profile() {
    let missing = CapabilityWorkerConfig::parse(&[
        OsString::from("--sandbox-profile"),
        OsString::from(LINUX_BWRAP_PROFILE),
    ])
    .expect_err("worker profile is mandatory");
    assert!(missing.contains("execution profile is required"));

    let local = CapabilityWorkerConfig::parse(&[
        OsString::from("--execution-profile"),
        OsString::from("local"),
        OsString::from("--sandbox-profile"),
        OsString::from(LINUX_BWRAP_PROFILE),
    ])
    .expect_err("ordinary local execution must not enter a worker");
    assert_eq!(
        local,
        "error[capability_worker.profile]: unsupported execution profile `local`"
    );

    let unknown = CapabilityWorkerConfig::parse(&[
        OsString::from("--execution-profile"),
        OsString::from("automatic"),
        OsString::from("--sandbox-profile"),
        OsString::from(LINUX_BWRAP_PROFILE),
    ])
    .expect_err("unknown execution profile must fail");
    assert!(unknown.contains("unsupported execution profile `automatic`"));

    let repeated = CapabilityWorkerConfig::parse(&[
        OsString::from("--execution-profile"),
        OsString::from("external-adapter"),
        OsString::from("--execution-profile"),
        OsString::from("crash-isolated"),
        OsString::from("--sandbox-profile"),
        OsString::from(LINUX_BWRAP_PROFILE),
    ])
    .expect_err("execution profile must be unique");
    assert!(repeated.contains("execution profile may be declared only once"));

    for (name, expected) in [
        (
            "external-adapter",
            NativeBoundaryExecutionProfile::ExternalAdapter,
        ),
        (
            "crash-isolated",
            NativeBoundaryExecutionProfile::CrashIsolated,
        ),
        (
            "cross-boundary",
            NativeBoundaryExecutionProfile::CrossBoundary,
        ),
    ] {
        let config = CapabilityWorkerConfig::parse(&[
            OsString::from("--execution-profile"),
            OsString::from(name),
            OsString::from("--sandbox-profile"),
            OsString::from(LINUX_BWRAP_PROFILE),
        ])
        .expect("closed worker profile");
        assert_eq!(config.execution_profile, expected);
    }
}

/// Rejects a manifest operation when the startup allowlist grants no capability.
#[test]
fn worker_denies_missing_capability_before_adapter_dispatch() {
    let output = run_frames(
        Vec::new(),
        concat!(
            "{\"type\":\"call\",\"version\":2,\"request_id\":1,\"owner_id\":7,",
            "\"capability\":\"postgres\",",
            "\"operation\":\"std.db.postgres.string\",\"arguments\":[]}\n",
            "{\"type\":\"shutdown\",\"version\":2}\n"
        ),
    );
    let reply = first_reply(&output);

    assert_eq!(
        reply["outcome"]["code"],
        "native_boundary.capability_denied"
    );
}

/// Rejects a request capability that does not own its manifest operation.
#[test]
fn worker_rejects_capability_operation_identity_mismatch() {
    let output = run_frames(
        vec!["--allow", "postgres", "--worker-class", "fast"],
        concat!(
            "{\"type\":\"call\",\"version\":2,\"request_id\":1,\"owner_id\":7,",
            "\"capability\":\"filesystem\",",
            "\"operation\":\"std.db.postgres.string\",\"arguments\":[]}\n",
            "{\"type\":\"shutdown\",\"version\":2}\n"
        ),
    );
    let reply = first_reply(&output);

    assert_eq!(
        reply["outcome"]["code"],
        "capability_worker.capability_mismatch"
    );
}

/// Rejects a granted capability when its scheduler class was not admitted.
#[test]
fn worker_denies_missing_scheduler_class_before_adapter_dispatch() {
    let output = run_frames(
        vec!["--allow", "postgres"],
        concat!(
            "{\"type\":\"call\",\"version\":2,\"request_id\":1,\"owner_id\":7,",
            "\"capability\":\"postgres\",",
            "\"operation\":\"std.db.postgres.string\",\"arguments\":[]}\n",
            "{\"type\":\"shutdown\",\"version\":2}\n"
        ),
    );
    let reply = first_reply(&output);

    assert_eq!(reply["outcome"]["code"], "native_boundary.scheduler_denied");
}

/// Allows policy admission and then reports the adapter's typed stale handle.
#[test]
fn worker_dispatches_only_after_capability_and_class_admission() {
    let output = run_frames(
        vec!["--allow", "postgres", "--worker-class", "fast"],
        concat!(
            "{\"type\":\"call\",\"version\":2,\"request_id\":1,\"owner_id\":7,",
            "\"capability\":\"postgres\",",
            "\"operation\":\"std.db.postgres.string\",\"arguments\":[",
            "{\"type\":\"handle\",\"value\":{\"id\":1,\"generation\":1}},",
            "{\"type\":\"text\",\"value\":\"name\"}]}\n",
            "{\"type\":\"shutdown\",\"version\":2}\n"
        ),
    );
    let reply = first_reply(&output);

    assert_eq!(reply["outcome"]["code"], "resource.stale_handle");
    assert_eq!(reply["reserved_credits"], 0);
}

/// Denies operations absent from the worker's declared manifest.
#[test]
fn worker_rejects_undeclared_operations() {
    let output = run_frames(
        Vec::new(),
        concat!(
            "{\"type\":\"call\",\"version\":2,\"request_id\":1,\"owner_id\":7,",
            "\"capability\":\"encoding\",",
            "\"operation\":\"std.encoding.base64.encode\",\"arguments\":[]}\n",
            "{\"type\":\"shutdown\",\"version\":2}\n"
        ),
    );
    let reply = first_reply(&output);

    assert_eq!(
        reply["outcome"]["code"],
        "capability_worker.operation_denied"
    );
}

/// Bounds input before JSON parsing and output during serialization.
#[test]
fn framing_rejects_oversized_input_and_output() {
    let mut input = Cursor::new(b"123456789\n");
    let error = read_json_frame::<CapabilityResponse>(&mut input, 8).expect_err("oversized input");
    assert!(error.contains("payload_limit"));

    let response = CapabilityResponse::Reply {
        version: CAPABILITY_PROTOCOL_VERSION,
        request_id: 1,
        reserved_credits: 0,
        available_credits: 1,
        outcome: CapabilityOutcome::Ok {
            value: CapabilityValue::Text("large response".repeat(32)),
        },
    };
    let error = write_json_frame(&mut Vec::new(), &response, 64).expect_err("oversized output");
    assert!(error.contains("reply_limit"));
}

/// Rejects wrong versions, zero identities, and lifetime request overflow.
#[test]
fn worker_rejects_invalid_protocol_lifecycle() {
    let wrong_version = run_capability_worker(
        test_config(&[]),
        Cursor::new(b"{\"type\":\"shutdown\",\"version\":3}\n"),
        Vec::new(),
    )
    .expect_err("wrong version");
    assert!(wrong_version.contains("capability_worker.version"));

    let zero_identity = run_capability_worker(
        test_config(&[]),
        Cursor::new(concat!(
            "{\"type\":\"call\",\"version\":2,\"request_id\":0,\"owner_id\":7,",
            "\"capability\":\"postgres\",",
            "\"operation\":\"std.db.postgres.string\",\"arguments\":[]}\n"
        )),
        Vec::new(),
    )
    .expect_err("zero request identity");
    assert!(zero_identity.contains("capability_worker.identity"));

    let request_limit = run_capability_worker(
        test_config(&["--max-requests", "1"]),
        Cursor::new(concat!(
            "{\"type\":\"call\",\"version\":2,\"request_id\":1,\"owner_id\":7,",
            "\"capability\":\"postgres\",",
            "\"operation\":\"denied\",\"arguments\":[]}\n",
            "{\"type\":\"call\",\"version\":2,\"request_id\":2,\"owner_id\":7,",
            "\"capability\":\"postgres\",",
            "\"operation\":\"denied\",\"arguments\":[]}\n"
        )),
        Vec::new(),
    )
    .expect_err("request lifetime bound");
    assert!(request_limit.contains("capability_worker.request_limit"));
}

/// Acknowledges cancellation without reviving an unknown or completed request.
#[test]
fn worker_rejects_stale_cancellation_without_failing_the_transport() {
    let output = run_frames(
        Vec::new(),
        concat!(
            "{\"type\":\"cancel\",\"version\":2,\"request_id\":9,\"owner_id\":7}\n",
            "{\"type\":\"shutdown\",\"version\":2}\n"
        ),
    );
    let acknowledgement = first_reply(&output);

    assert_eq!(acknowledgement["type"], "cancel_ack");
    assert_eq!(acknowledgement["request_id"], 9);
    assert_eq!(acknowledgement["accepted"], false);
}

/// Executes one compiler-intrinsic filesystem operation under its closed capability.
#[test]
fn worker_admits_declared_filesystem_operation() {
    let requests = [
        CapabilityRequest::Call {
            version: CAPABILITY_PROTOCOL_VERSION,
            request_id: 1,
            owner_id: 7,
            capability: "filesystem".to_string(),
            operation: "std.io.file.exists".to_string(),
            arguments: vec![CapabilityValue::Text("/".to_string())],
        },
        CapabilityRequest::Shutdown {
            version: CAPABILITY_PROTOCOL_VERSION,
        },
    ];
    let mut input = Vec::new();
    for request in requests {
        write_json_frame(&mut input, &request, 4_096).expect("request frame");
    }
    let mut output = Vec::new();
    run_capability_worker(
        test_config(&["--allow", "filesystem", "--credit-limit", "1"]),
        Cursor::new(input),
        &mut output,
    )
    .expect("filesystem worker run");
    let responses = String::from_utf8(output).expect("response frames");
    let replies = responses
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("response JSON"))
        .collect::<Vec<_>>();
    assert_eq!(replies[0]["outcome"]["status"], "ok");
    assert_eq!(replies[0]["outcome"]["value"]["type"], "bool");
    assert_eq!(replies[0]["outcome"]["value"]["value"], true);
    assert_eq!(replies[1]["type"], "shutdown_ack");
}

/// Proves a cancel frame is consumed while adapter execution is still polling.
#[test]
fn worker_delivers_cooperative_cancellation_during_adapter_execution() {
    let observed = Arc::new(AtomicBool::new(false));
    let executor = PollingExecutor {
        observed: observed.clone(),
    };
    let requests = [
        CapabilityRequest::Call {
            version: CAPABILITY_PROTOCOL_VERSION,
            request_id: 1,
            owner_id: 7,
            capability: "postgres".to_string(),
            operation: "std.db.postgres.connect".to_string(),
            arguments: Vec::new(),
        },
        CapabilityRequest::Cancel {
            version: CAPABILITY_PROTOCOL_VERSION,
            request_id: 1,
            owner_id: 8,
        },
        CapabilityRequest::Cancel {
            version: CAPABILITY_PROTOCOL_VERSION,
            request_id: 1,
            owner_id: 7,
        },
        CapabilityRequest::Shutdown {
            version: CAPABILITY_PROTOCOL_VERSION,
        },
    ];
    let mut input = Vec::new();
    for request in requests {
        write_json_frame(&mut input, &request, 4096).expect("request frame");
    }
    let mut output = Vec::new();

    run_with_executor(
        test_config(&["--credit-limit", "1"]),
        Cursor::new(input),
        &mut output,
        executor,
    )
    .expect("cooperative worker run");

    let responses = String::from_utf8(output).expect("response frames");
    let responses = responses
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("response JSON"))
        .collect::<Vec<_>>();
    assert_eq!(responses[0]["type"], "cancel_ack");
    assert_eq!(responses[0]["accepted"], false);
    assert_eq!(responses[1]["type"], "cancel_ack");
    assert_eq!(responses[1]["accepted"], true);
    assert_eq!(responses[2]["outcome"]["code"], "native_boundary.cancelled");
    assert_eq!(responses[2]["reserved_credits"], 0);
    assert_eq!(responses[3]["type"], "shutdown_ack");
    assert!(observed.load(Ordering::Acquire));
}

/// Proves a protocol failure cancels and drains already admitted adapter work.
#[test]
fn worker_protocol_failure_cancels_active_adapter_before_returning() {
    let observed = Arc::new(AtomicBool::new(false));
    let requests = [
        CapabilityRequest::Call {
            version: CAPABILITY_PROTOCOL_VERSION,
            request_id: 1,
            owner_id: 7,
            capability: "postgres".to_string(),
            operation: "std.db.postgres.connect".to_string(),
            arguments: Vec::new(),
        },
        CapabilityRequest::Shutdown { version: 3 },
    ];
    let mut input = Vec::new();
    for request in requests {
        write_json_frame(&mut input, &request, 4096).expect("request frame");
    }

    let error = run_with_executor(
        test_config(&["--credit-limit", "1"]),
        Cursor::new(input),
        Vec::new(),
        PollingExecutor {
            observed: observed.clone(),
        },
    )
    .expect_err("invalid version");

    assert!(error.contains("capability_worker.version"));
    assert!(observed.load(Ordering::Acquire));
}

/// Test executor that remains in adapter work until its token is cancelled.
struct PollingExecutor {
    observed: Arc<AtomicBool>,
}

impl CapabilityExecutor for PollingExecutor {
    fn call(
        &mut self,
        call: CapabilityCall,
        cancellation: &NativeBoundaryCancellationToken,
    ) -> NativeBoundaryWorkerReply {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !cancellation.is_cancelled() && Instant::now() < deadline {
            std::thread::yield_now();
        }
        let result = if cancellation.is_cancelled() {
            self.observed.store(true, Ordering::Release);
            let error = error_for(ErrorKind::Cancelled);
            NativeBoundaryReplyTerm::Error {
                code: error.code.to_string(),
                message: error.message.to_string(),
                offset: 0,
            }
        } else {
            NativeBoundaryReplyTerm::Error {
                code: "test.cancellation_timeout".to_string(),
                message: "Cancellation did not reach polling adapter work.".to_string(),
                offset: 0,
            }
        };
        test_worker_reply(call.request_id, result)
    }

    fn dispose(&mut self, dispose: CapabilityDispose) -> NativeBoundaryWorkerReply {
        test_worker_reply(
            dispose.request_id,
            NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Unit),
        )
    }
}

/// Builds a deterministic worker reply for an injected protocol executor.
fn test_worker_reply(
    request_id: u64,
    result: NativeBoundaryReplyTerm,
) -> NativeBoundaryWorkerReply {
    NativeBoundaryWorkerReply {
        request_id: RequestId { value: request_id },
        result,
        reserved_credits: 0,
        available_credits: 1,
    }
}

/// Runs protocol frames with compact string arguments.
fn run_frames(args: Vec<&str>, frames: &str) -> String {
    let args = [
        "--execution-profile",
        "external-adapter",
        "--sandbox-profile",
        LINUX_BWRAP_PROFILE,
    ]
    .into_iter()
    .chain(args)
    .map(OsString::from)
    .collect::<Vec<_>>();
    let config = CapabilityWorkerConfig::parse(&args).expect("worker config");
    let mut output = Vec::new();
    run_capability_worker(config, Cursor::new(frames.as_bytes()), &mut output)
        .expect("worker frames");
    String::from_utf8(output).expect("UTF-8 frames")
}

/// Parses a test policy while retaining the production sandbox requirement.
fn test_config(args: &[&str]) -> CapabilityWorkerConfig {
    let args = [
        "--execution-profile",
        "crash-isolated",
        "--sandbox-profile",
        LINUX_BWRAP_PROFILE,
    ]
    .into_iter()
    .chain(args.iter().copied())
    .map(OsString::from)
    .collect::<Vec<_>>();
    CapabilityWorkerConfig::parse(&args).expect("sandboxed worker config")
}

/// Parses the first response frame as JSON.
fn first_reply(output: &str) -> Value {
    serde_json::from_str(output.lines().next().expect("first reply")).expect("reply JSON")
}
