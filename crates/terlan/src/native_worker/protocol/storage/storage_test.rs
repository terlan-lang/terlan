//! Worker-side storage decoding, cancellation, and authority tests.

use super::*;
use crate::terlan_native_boundary::metadata::NativeBoundaryExecutionProfile;

fn executor(path: &std::path::Path) -> StorageExecutor {
    StorageExecutor {
        path: Some(path.to_owned()),
        database: None,
        max_frame_bytes: 4096,
    }
}

fn checkpoint(payload: &[u8]) -> CapabilityValue {
    CapabilityValue::Tuple(vec![
        CapabilityValue::Text("checkpoint".into()),
        CapabilityValue::Int(1),
        CapabilityValue::Int(1),
        CapabilityValue::Bytes(payload.to_vec()),
    ])
}

fn append(payload: &[u8]) -> Vec<CapabilityValue> {
    vec![
        CapabilityValue::Int(0),
        CapabilityValue::List(vec![checkpoint(payload)]),
    ]
}

#[test]
fn storage_worker_reopens_replays_and_rejects_conflicting_checkpoints() {
    let directory = tempfile::tempdir().expect("private database directory");
    let path = directory.path().join("checkpoints.sqlite");
    let mut first = executor(&path);
    let CapabilityValue::Tuple(opened) = first.execute("runtime.storage.open", vec![]).unwrap()
    else {
        panic!("instance observation")
    };
    assert!(
        matches!(&opened[0], CapabilityValue::Bytes(identity) if identity.len() == 32 && identity != &[0; 32])
    );
    assert_eq!(
        first
            .execute("runtime.storage.append", append(b"state"))
            .unwrap(),
        CapabilityValue::Bool(true)
    );
    drop(first);
    let mut second = executor(&path);
    let CapabilityValue::Tuple(reopened) = second.execute("runtime.storage.open", vec![]).unwrap()
    else {
        panic!("instance observation")
    };
    assert_eq!(opened[0], reopened[0]);
    assert_eq!(
        reopened[1],
        CapabilityValue::Tuple(vec![
            CapabilityValue::Int(1),
            CapabilityValue::Int(1),
            CapabilityValue::Int(0)
        ])
    );
    assert_eq!(
        second.execute("runtime.storage.sequence", vec![]).unwrap(),
        CapabilityValue::Int(1)
    );
    assert_eq!(
        second
            .execute("runtime.storage.append", append(b"state"))
            .unwrap(),
        CapabilityValue::Bool(false)
    );
    assert!(matches!(
        second.execute("runtime.storage.append", append(b"conflict")),
        Err(StorageError::Conflict)
    ));
    assert_eq!(
        second
            .execute(
                "runtime.storage.load",
                vec![CapabilityValue::Text("checkpoint".into())]
            )
            .unwrap(),
        CapabilityValue::List(vec![checkpoint(b"state")])
    );
    assert_eq!(
        second
            .execute("runtime.storage.compact", vec![CapabilityValue::Int(2)])
            .unwrap(),
        CapabilityValue::Tuple(vec![
            CapabilityValue::Int(1),
            CapabilityValue::Int(0),
            CapabilityValue::Int(1)
        ])
    );
    assert_eq!(
        second.execute("runtime.storage.sequence", vec![]).unwrap(),
        CapabilityValue::Int(1)
    );
}

#[test]
fn storage_worker_rejects_shapes_before_creating_a_database() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("checkpoints.sqlite");
    let mut storage = executor(&path);
    assert!(storage
        .execute("runtime.storage.open", vec![CapabilityValue::Unit])
        .is_err());
    assert!(!path.exists());
    assert!(storage
        .execute("runtime.storage.flush", vec![CapabilityValue::Unit])
        .is_err());
    assert!(!path.exists());
    for (expected, next) in [(0, 1), (1, 1), (2, 1), (1, i64::MAX)] {
        assert!(matches!(
            storage.execute(
                "runtime.storage.migrate_schema",
                vec![CapabilityValue::Int(expected), CapabilityValue::Int(next)]
            ),
            Err(StorageError::Invalid(_))
        ));
        assert!(!path.exists());
    }
    for arguments in [
        vec![
            CapabilityValue::Int(-1),
            CapabilityValue::List(vec![checkpoint(b"state")]),
        ],
        vec![
            CapabilityValue::Int(0),
            CapabilityValue::List(vec![checkpoint(b"state"), CapabilityValue::Unit]),
        ],
        vec![
            CapabilityValue::Int(0),
            CapabilityValue::List(vec![checkpoint(b"state")]),
            CapabilityValue::Unit,
        ],
    ] {
        assert!(matches!(
            storage.execute("runtime.storage.append", arguments),
            Err(StorageError::Invalid(_))
        ));
        assert!(!path.exists());
    }
    assert!(storage
        .execute(
            "runtime.storage.sequence",
            vec![CapabilityValue::Text("/host/path".into())]
        )
        .is_err());
    assert!(!path.exists());
}

#[test]
fn storage_worker_flush_returns_the_committed_boundary_and_redacts_busy_errors() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("flush.sqlite");
    let mut storage = executor(&path);
    storage
        .execute("runtime.storage.append", append(b"state"))
        .unwrap();
    assert_eq!(
        storage.execute("runtime.storage.flush", vec![]).unwrap(),
        CapabilityValue::Tuple(vec![
            CapabilityValue::Int(1),
            CapabilityValue::Int(1),
            CapabilityValue::Int(0)
        ])
    );
    let NativeBoundaryReplyTerm::Ok(failure) = error_reply(StorageError::Busy) else {
        panic!("typed domain failure")
    };
    assert_eq!(
        crate::terlan_native_boundary::storage_reply::StorageFailure::from_term(&failure).unwrap(),
        Some(crate::terlan_native_boundary::storage_reply::StorageFailure::Busy)
    );
}

#[test]
fn storage_worker_rejects_unreadable_replies_before_commit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("checkpoints.sqlite");
    let mut storage = executor(&path);
    assert!(matches!(
        storage.execute("runtime.storage.append", append(&[255; 4096])),
        Err(StorageError::Invalid(_))
    ));
    assert!(!path.exists());
}

#[test]
fn storage_worker_cancellation_and_duplicate_ids_preserve_credits() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("checkpoints.sqlite");
    let mut storage = executor(&path);
    let mut worker = NativeBoundaryWorker::new(1);
    let cancellation = NativeBoundaryCancellationToken::new();
    cancellation.cancel();
    let call = || CapabilityCall {
        request_id: 1,
        owner_id: 1,
        capability: "storage".into(),
        operation: "runtime.storage.append".into(),
        arguments: append(b"state"),
    };
    let reply = storage.call(&mut worker, call(), &cancellation);
    assert!(matches!(
        reply.result,
        NativeBoundaryReplyTerm::Error { .. }
    ));
    assert_eq!(reply.reserved_credits, 0);
    assert_eq!(reply.available_credits, 1);
    assert!(!path.exists());
    let reply = storage.call(&mut worker, call(), &NativeBoundaryCancellationToken::new());
    assert!(matches!(
        reply.result,
        NativeBoundaryReplyTerm::Error { .. }
    ));
    assert_eq!(reply.reserved_credits, 0);
    assert!(!path.exists());
}

#[test]
fn storage_worker_cannot_create_storage_without_startup_binding() {
    let config = CapabilityWorkerConfig {
        execution_profile: NativeBoundaryExecutionProfile::CrashIsolated,
        sandbox_profile: crate::terlan_native_boundary::capability_sandbox::CapabilitySandboxProfile::LinuxBwrapV1,
        capabilities: Default::default(), worker_classes: Default::default(),
        max_payload_bytes: 4096, max_requests: 10, credit_limit: 1, storage_database: false,
    };
    assert!(matches!(
        StorageExecutor::new(&config).execute("runtime.storage.sequence", vec![]),
        Err(StorageError::Invalid(_))
    ));
}
