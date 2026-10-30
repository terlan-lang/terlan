//! Real sandboxed SQLite worker transport, scheduler wakeup, and restart checks.

use super::*;
use crate::runtime::vm::distributed_state::{
    VmDistributedStateEntry, VmDistributedStatePolicy, VmDistributedStateScope,
    VmDistributedStateStore, VmDistributedStateVersion,
};
use crate::runtime::vm::term_format::{decode_tetf_checkpoint, encode_tetf_checkpoint};
use crate::runtime::vm::ReplValue;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

fn logical_state() -> Vec<VmDistributedStateEntry> {
    vec![VmDistributedStateEntry {
        scope: VmDistributedStateScope::new("cart", "alice").unwrap(),
        owner_node_id: "node-a".into(),
        version: VmDistributedStateVersion::new(7, "node-a").unwrap(),
        policy: VmDistributedStatePolicy::LastWriterWins,
        value: ReplValue::Tuple(vec![
            ReplValue::Map(vec![(
                ReplValue::String("quantity".into()),
                ReplValue::Int(42),
            )]),
            ReplValue::Set(vec![ReplValue::Int(1), ReplValue::Int(2)]),
            ReplValue::BitString(
                crate::runtime::vm::bitstring::VmBitString::from_bytes(&[0xa0], 3).unwrap(),
            ),
            ReplValue::Atom("ready".into()),
        ]),
    }]
}

fn storage_call(
    client: &mut VmCapabilityWorkerClient,
    operation: &str,
    arguments: Vec<NativeBoundaryTerm>,
) -> NativeBoundaryReplyTerm {
    let (mut processes, mut scheduler, mut timers, owner) = runtime();
    client
        .start_call(
            &mut VmCapabilityWorkerRuntime {
                timers: &mut timers,
                processes: &mut processes,
                scheduler: &mut scheduler,
            },
            VmCapabilityWorkerCall {
                owner,
                context: request_context("storage"),
                operation: operation.into(),
                arguments,
                now_tick: 0,
                timeout_ticks: 100,
            },
        )
        .expect("park storage call");
    let completion = wait_for_completion(client, &mut timers, &mut processes, &mut scheduler);
    assert_eq!(
        processes.get(owner).unwrap().state,
        VmProcessState::Runnable
    );
    let VmCapabilityWorkerCompletion::Reply { reply, .. } = completion else {
        panic!("unexpected storage completion: {completion:?}");
    };
    reply
}

/// Exercises the actual executable and sandbox; never skips missing prerequisites internally.
#[test]
#[ignore = "requires a separately built terlan-native-worker executable and Linux sandbox"]
fn capability_storage_worker_survives_restart_with_durable_checkpoint() {
    let executable = std::env::var_os("TERLAN_TEST_CAPABILITY_WORKER").expect("built worker path");
    storage_checkpoint_probe(PathBuf::from(executable));
}

#[test]
fn packaged_application_hosts_sandboxed_storage_worker_with_durable_checkpoint() {
    let directory = tempfile::tempdir().unwrap();
    storage_checkpoint_probe(packaged_application(directory.path()));
}

#[test]
fn pinned_packaged_storage_worker_survives_executable_unlink() {
    const CHILD: &str = "TERLAN_TEST_PINNED_APPLICATION_WORKER";
    if std::env::var(CHILD).as_deref() != Ok("1") {
        let name = concat!(
            module_path!(),
            "::pinned_packaged_storage_worker_survives_executable_unlink"
        );
        // The child repackages the VM and starts a sandboxed storage worker while
        // the parent runtime shard is saturated. Keep hangs bounded without
        // treating normal hosted-runner contention as a protocol failure.
        let output = terlan_process_owner::ProcessControl::new(Duration::from_secs(120))
            .capture_stdout(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        name.strip_prefix("terlan::").unwrap(),
                        "--nocapture",
                    ])
                    .env(CHILD, "1"),
                65_536,
                |_| Ok(()),
            )
            .unwrap();
        assert!(String::from_utf8_lossy(&output).contains("1 passed; 0 failed"));
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let executable = packaged_application(directory.path());
    let mut application = crate::runtime::executable::EmbeddedApplication::open(&executable)
        .unwrap()
        .unwrap();
    let files = application.materialize().unwrap();
    application.pin_for_workers(&files).unwrap();
    std::fs::remove_file(executable).unwrap();
    storage_checkpoint_probe(
        crate::runtime::executable::pinned_worker_path()
            .unwrap()
            .unwrap(),
    );
}

fn packaged_application(directory: &Path) -> PathBuf {
    let test_executable = std::env::current_exe().unwrap();
    let runtime = std::env::var_os("TERLAN_VM_RUNNER")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            test_executable
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("terlan-vm")
        });
    assert!(
        runtime.is_file(),
        "build the compiler-paired terlan-vm before running application packaging tests"
    );
    let application = directory.join("application with spaces");
    std::fs::copy(runtime, &application).unwrap();
    let image = directory.join("unused.tvm");
    // The worker role must never execute the application's entry image.
    std::fs::write(
        &image,
        b"this is deliberately not an executable native image",
    )
    .unwrap();
    crate::runtime::executable::append_application(
        &application,
        crate::runtime::executable::ApplicationEntry::Cli {
            image: "vm/main.tvm".into(),
            function: "app.Main.main".into(),
        },
        &[("vm/main.tvm".into(), image)],
        vec![],
    )
    .unwrap();
    application
}

fn storage_checkpoint_probe(executable: PathBuf) {
    let durable = tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .expect("durable directory");
    let mut policy =
        VmCapabilityWorkerPolicy::new(executable, NativeBoundaryExecutionProfile::CrashIsolated)
            .unwrap()
            .allow("storage")
            .admit_worker_class("blocking")
            .with_credit_limit(1)
            .unwrap();
    policy.storage_directory = Some(durable.path().canonicalize().unwrap());
    let atoms = ["ready".to_string()];
    let state = logical_state();
    let payload = encode_tetf_checkpoint(&state, &atoms, 4096).unwrap();
    let checkpoint = NativeBoundaryTerm::Tuple(vec![
        NativeBoundaryTerm::Text("checkpoint".into()),
        NativeBoundaryTerm::Int(1),
        NativeBoundaryTerm::Int(1),
        NativeBoundaryTerm::Bytes(payload),
    ]);
    let append = vec![
        NativeBoundaryTerm::Int(0),
        NativeBoundaryTerm::List(vec![checkpoint.clone()]),
    ];
    let mut first =
        VmCapabilityWorkerClient::spawn(worker_identity_for(1), policy.clone()).unwrap();
    let NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Tuple(opened)) =
        storage_call(&mut first, "runtime.storage.open", vec![])
    else {
        panic!("open instance observation")
    };
    assert!(
        matches!(&opened[0], NativeBoundaryTerm::Bytes(identity) if identity.len() == 32 && identity != &[0; 32])
    );
    assert_eq!(
        storage_call(&mut first, "runtime.storage.append", append.clone()),
        NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Bool(true))
    );
    let schema_status = NativeBoundaryTerm::Tuple(vec![
        NativeBoundaryTerm::Int(1),
        NativeBoundaryTerm::Int(2),
        NativeBoundaryTerm::Int(1),
    ]);
    assert_eq!(
        storage_call(
            &mut first,
            "runtime.storage.migrate_schema",
            vec![NativeBoundaryTerm::Int(1), NativeBoundaryTerm::Int(2)]
        ),
        NativeBoundaryReplyTerm::Ok(schema_status.clone())
    );
    assert_eq!(
        storage_call(&mut first, "runtime.storage.flush", vec![]),
        NativeBoundaryReplyTerm::Ok(schema_status.clone())
    );
    drop(first);
    assert!(durable.path().join("checkpoints.sqlite").is_file());
    let mut second = VmCapabilityWorkerClient::spawn(worker_identity_for(2), policy).unwrap();
    let NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Tuple(reopened)) =
        storage_call(&mut second, "runtime.storage.open", vec![])
    else {
        panic!("reopen instance observation")
    };
    assert_eq!(
        opened[0], reopened[0],
        "logical identity survives a complete worker restart"
    );
    assert_eq!(reopened[1], schema_status);
    assert_eq!(
        storage_call(&mut second, "runtime.storage.status", vec![]),
        NativeBoundaryReplyTerm::Ok(schema_status)
    );
    let NativeBoundaryReplyTerm::Ok(failure) = storage_call(
        &mut second,
        "runtime.storage.migrate_schema",
        vec![NativeBoundaryTerm::Int(1), NativeBoundaryTerm::Int(3)],
    ) else {
        panic!("typed schema failure")
    };
    assert_eq!(
        crate::terlan_native_boundary::storage_reply::StorageFailure::from_term(&failure).unwrap(),
        Some(
            crate::terlan_native_boundary::storage_reply::StorageFailure::Schema {
                expected: 1,
                actual: 2
            }
        )
    );
    assert_eq!(
        storage_call(&mut second, "runtime.storage.sequence", vec![]),
        NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Int(1))
    );
    let loaded = storage_call(
        &mut second,
        "runtime.storage.load",
        vec![NativeBoundaryTerm::Text("checkpoint".into())],
    );
    assert_eq!(
        loaded,
        NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::List(vec![checkpoint]))
    );
    let NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::List(rows)) = loaded else {
        panic!("expected stored checkpoint");
    };
    let NativeBoundaryTerm::Tuple(columns) = &rows[0] else {
        panic!("expected checkpoint fields");
    };
    let NativeBoundaryTerm::Bytes(payload) = &columns[3] else {
        panic!("expected encoded logical state");
    };
    let restored =
        VmDistributedStateStore::import_snapshot(decode_tetf_checkpoint(payload, &atoms).unwrap())
            .unwrap();
    assert_eq!(restored.export_snapshot(), state);
    assert!(
        decode_tetf_checkpoint(payload, &[]).is_err(),
        "current image must admit restored atoms"
    );
    assert_eq!(
        storage_call(&mut second, "runtime.storage.append", append),
        NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Bool(false))
    );
    assert!(matches!(
        storage_call(
            &mut second,
            "std.io.file.read_text",
            vec![NativeBoundaryTerm::Text("/etc/passwd".into())]
        ),
        NativeBoundaryReplyTerm::Error { .. }
    ));
}
