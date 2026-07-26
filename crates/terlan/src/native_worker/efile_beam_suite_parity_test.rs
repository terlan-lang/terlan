//! Portable replacement coverage for OTP's retired `efile_SUITE`.

use std::ffi::OsString;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::{run_capability_worker, CapabilityWorkerConfig};
use crate::terlan_native_boundary::capability_sandbox::{
    CapabilitySandboxLimits, LINUX_BWRAP_PROFILE,
};
use crate::terlan_native_boundary::capability_wire::{
    read_json_frame, write_json_frame, CapabilityOutcome, CapabilityRequest, CapabilityResponse,
    CapabilityValue, CAPABILITY_PROTOCOL_VERSION,
};

const FRAME_LIMIT: usize = 64 * 1024;
const EFILE_REPETITIONS: usize = 10;
const EFILE_OPEN_LIMIT: usize = 64;
const PROC_READ_REPETITIONS: usize = 500;
static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(1);

/// A temporary UTF-8 fixture removed even when a parity assertion fails.
struct TempFile {
    path: PathBuf,
}

impl TempFile {
    /// Creates one unique file without sharing a persistent test directory.
    fn create(contents: &str) -> Self {
        let sequence = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "terlan-efile-parity-{}-{sequence}.txt",
            std::process::id()
        ));
        std::fs::write(&path, contents).expect("write efile parity fixture");
        Self { path }
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Replaces repeated raw-driver descriptor exhaustion with the Terlan
/// filesystem contract: every operation is request-scoped, bounded by the
/// worker's fixed descriptor envelope, and returns all transport credits.
#[test]
fn efile_suite_repeated_reads_release_descriptors_and_recover_worker_capacity() {
    let fixture = TempFile::create("request-scoped file payload\n");
    let request_count = EFILE_REPETITIONS * EFILE_OPEN_LIMIT;
    let descriptors_before = open_descriptor_count();
    let replies = run_read_text_requests(&fixture.path, request_count);
    let descriptors_after = open_descriptor_count();

    assert_eq!(
        CapabilitySandboxLimits::linux_default().open_files,
        EFILE_OPEN_LIMIT as u64
    );
    assert_eq!(replies.len(), request_count + 1);
    for (index, reply) in replies[..request_count].iter().enumerate() {
        assert_text_reply(reply, index + 1, "request-scoped file payload\n");
    }
    assert!(matches!(
        replies.last(),
        Some(CapabilityResponse::ShutdownAck {
            version: CAPABILITY_PROTOCOL_VERSION
        })
    ));
    assert_eq!(
        descriptors_after, descriptors_before,
        "request-scoped filesystem calls leaked host descriptors"
    );
}

/// Preserves OTP's zero-sized pseudo-file regression through the real
/// filesystem capability operation. Linux proc metadata reports zero bytes,
/// but each request must still read to EOF and return a non-empty value.
#[test]
fn efile_suite_reads_zero_sized_proc_file_repeatedly_without_empty_results() {
    let proc_file = Path::new("/proc/self/status");
    assert_eq!(
        std::fs::metadata(proc_file)
            .expect("proc status metadata")
            .len(),
        0
    );
    let expected = std::fs::read_to_string(proc_file).expect("proc status probe");
    assert!(!expected.is_empty());

    let replies = run_read_text_requests(proc_file, PROC_READ_REPETITIONS);
    assert_eq!(replies.len(), PROC_READ_REPETITIONS + 1);
    for (index, reply) in replies[..PROC_READ_REPETITIONS].iter().enumerate() {
        let CapabilityResponse::Reply {
            version,
            request_id,
            reserved_credits,
            available_credits,
            outcome:
                CapabilityOutcome::Ok {
                    value: CapabilityValue::Text(value),
                },
        } = reply
        else {
            panic!("request {} did not return text: {reply:?}", index + 1);
        };
        assert_eq!(*version, CAPABILITY_PROTOCOL_VERSION);
        assert_eq!(*request_id, (index + 1) as u64);
        assert_eq!(
            *reserved_credits + *available_credits,
            EFILE_OPEN_LIMIT as u64
        );
        assert!(!value.is_empty());
        assert!(value.starts_with("Name:"));
    }
    assert!(matches!(
        &replies[PROC_READ_REPETITIONS - 1],
        CapabilityResponse::Reply {
            reserved_credits: 0,
            available_credits,
            ..
        } if *available_credits == EFILE_OPEN_LIMIT as u64
    ));
}

/// Runs real filesystem dispatch behind the bounded capability-worker
/// coordinator and returns every typed response frame.
fn run_read_text_requests(path: &Path, request_count: usize) -> Vec<CapabilityResponse> {
    let mut replies = Vec::with_capacity(request_count + 1);
    let mut first_request_id = 1;
    while first_request_id <= request_count {
        let batch_count = EFILE_OPEN_LIMIT.min(request_count - first_request_id + 1);
        let mut batch = run_read_text_batch(path, first_request_id, batch_count);
        let shutdown = batch.pop().expect("efile batch shutdown acknowledgement");
        replies.extend(batch);
        first_request_id += batch_count;
        if first_request_id > request_count {
            replies.push(shutdown);
        }
    }
    replies
}

/// Runs one request batch no larger than the worker's fixed capacity.
fn run_read_text_batch(
    path: &Path,
    first_request_id: usize,
    request_count: usize,
) -> Vec<CapabilityResponse> {
    assert!((1..=EFILE_OPEN_LIMIT).contains(&request_count));
    let request_limit = request_count.to_string();
    let credit_limit = EFILE_OPEN_LIMIT.to_string();
    let config = CapabilityWorkerConfig::parse(&[
        OsString::from("--execution-profile"),
        OsString::from("crash-isolated"),
        OsString::from("--sandbox-profile"),
        OsString::from(LINUX_BWRAP_PROFILE),
        OsString::from("--allow"),
        OsString::from("filesystem"),
        OsString::from("--max-payload-bytes"),
        OsString::from(FRAME_LIMIT.to_string()),
        OsString::from("--max-requests"),
        OsString::from(request_limit),
        OsString::from("--credit-limit"),
        OsString::from(credit_limit),
    ])
    .expect("efile capability-worker policy");
    let path = path.to_string_lossy().into_owned();
    let mut input = Vec::new();
    for request_id in first_request_id..first_request_id + request_count {
        write_json_frame(
            &mut input,
            &CapabilityRequest::Call {
                version: CAPABILITY_PROTOCOL_VERSION,
                request_id: request_id as u64,
                owner_id: 1,
                capability: "filesystem".to_string(),
                operation: "std.io.file.read_text".to_string(),
                arguments: vec![CapabilityValue::Text(path.clone())],
            },
            FRAME_LIMIT,
        )
        .expect("efile request frame");
    }
    write_json_frame(
        &mut input,
        &CapabilityRequest::Shutdown {
            version: CAPABILITY_PROTOCOL_VERSION,
        },
        FRAME_LIMIT,
    )
    .expect("efile shutdown frame");

    let mut output = Vec::new();
    run_capability_worker(config, Cursor::new(input), &mut output)
        .expect("efile capability-worker run");
    let mut output = Cursor::new(output);
    let mut replies = Vec::with_capacity(request_count + 1);
    while let Some(reply) =
        read_json_frame::<CapabilityResponse>(&mut output, FRAME_LIMIT).expect("efile reply frame")
    {
        replies.push(reply);
    }
    replies
}

/// Asserts one successful text reply and complete credit release.
fn assert_text_reply(reply: &CapabilityResponse, request_id: usize, expected: &str) {
    assert!(
        matches!(
        reply,
        CapabilityResponse::Reply {
            version: CAPABILITY_PROTOCOL_VERSION,
            request_id: actual_request_id,
            outcome: CapabilityOutcome::Ok {
                value: CapabilityValue::Text(value)
            },
            ..
        } if *actual_request_id == request_id as u64
            && value == expected
        ),
        "request {request_id} returned unexpected reply {reply:?}"
    );
}

/// Counts this process's currently open Linux file descriptors.
fn open_descriptor_count() -> usize {
    std::fs::read_dir("/proc/self/fd")
        .expect("open descriptor directory")
        .count()
}
