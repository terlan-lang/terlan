//! Direct-AOT managed allocation and continuation lifecycle assertions.

use std::path::Path;
use std::process::Command;

/// Runs managed allocation, park/resume, and child-fork scenarios in-shard.
pub(super) fn assert_direct_managed_execution(image_path: &Path, descriptor: &[u8]) {
    assert_managed_metadata_records(descriptor);
    for (entry, test_eval, context, failure) in [
        (
            "managed_constructed",
            true,
            "run managed allocation Terlan consumer",
            "terlan-vm could not allocate on the execution shard",
        ),
        (
            "managed_yielded",
            true,
            "run managed continuation Terlan consumer",
            "terlan-vm could not restore an execution-shard managed continuation",
        ),
        (
            "spawn_managed_child",
            true,
            "run managed child Terlan consumer",
            "terlan-vm could not isolate a spawned actor managed heap",
        ),
        (
            "managed_returned",
            false,
            "run managed result Terlan consumer",
            "terlan-vm could not materialize an execution-shard managed result",
        ),
        (
            "managed_entry_resume",
            false,
            "run managed entry and resume Terlan consumer",
            "terlan-vm could not retain an entry parameter across suspension",
        ),
        (
            "managed_branch_left",
            false,
            "run selected managed branch Terlan consumer",
            "terlan-vm could not restore the selected managed branch",
        ),
        (
            "managed_branch_right",
            false,
            "run fallback managed branch Terlan consumer",
            "terlan-vm could not restore the fallback managed branch",
        ),
        (
            "managed_nested",
            false,
            "run nested managed suspension Terlan consumer",
            "terlan-vm could not restore nested managed continuations",
        ),
        (
            "managed_repeated",
            false,
            "run repeated managed suspension Terlan consumer",
            "terlan-vm could not restore repeated managed continuations",
        ),
        (
            "managed_tail",
            false,
            "run tail managed suspension Terlan consumer",
            "terlan-vm could not forward a tail managed continuation",
        ),
        (
            "managed_non_tail",
            false,
            "run non-tail managed suspension Terlan consumer",
            "terlan-vm could not compose a managed continuation result",
        ),
        (
            "managed_non_tail_repeated",
            false,
            "run repeated non-tail managed suspension Terlan consumer",
            "terlan-vm could not compose repeated managed continuations",
        ),
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_terlan-vm"));
        command.arg("run").arg(image_path).arg("--entry").arg(entry);
        if test_eval {
            command.arg("--test-eval");
        }
        let output = command
            .env_remove("TERLAN_NATIVE_WORKER")
            .output()
            .unwrap_or_else(|error| panic!("{context}: {error}"));
        assert!(
            output.status.success(),
            "{failure}:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// Proves the built image carries the canonical fixed-layout registry used at runtime.
fn assert_managed_metadata_records(descriptor: &[u8]) {
    let record_count = u16::from_le_bytes(
        descriptor[14..16]
            .try_into()
            .expect("descriptor record count"),
    ) as usize;
    let digest_offset = descriptor.len() - 32;
    let mut offset = 32;
    let mut managed = None;
    let mut collections = None;
    let mut atoms = None;
    for _ in 0..record_count {
        let kind = u16::from_le_bytes(
            descriptor[offset..offset + 2]
                .try_into()
                .expect("descriptor record kind"),
        );
        let length = u32::from_le_bytes(
            descriptor[offset + 4..offset + 8]
                .try_into()
                .expect("descriptor record length"),
        ) as usize;
        let start = offset + 8;
        let end = start + length;
        assert!(
            end <= digest_offset,
            "descriptor record exceeds its envelope"
        );
        if kind == 10 {
            managed = Some(&descriptor[start..end]);
        }
        if kind == 11 {
            collections = Some(&descriptor[start..end]);
        }
        if kind == 12 {
            atoms = Some(&descriptor[start..end]);
        }
        offset = end;
    }
    assert_eq!(
        offset, digest_offset,
        "descriptor records must fill envelope"
    );
    let managed = managed.expect("managed aggregate layout record");
    let count = u16::from_le_bytes(managed[..2].try_into().expect("managed layout count"));
    assert!(count >= 1, "managed layout record must contain Pair");
    assert_eq!(
        &managed[22..26],
        b"TVMA",
        "managed layout payload must use canonical aggregate ABI"
    );
    let collections = collections.expect("managed collection schema record");
    let count = u16::from_le_bytes(
        collections[..2]
            .try_into()
            .expect("managed collection count"),
    );
    assert!(
        count >= 3,
        "managed collection record must contain List, Map, and Set"
    );
    assert_eq!(
        &collections[22..26],
        b"TVCL",
        "managed collection payload must use canonical collection ABI"
    );
    let atoms = atoms.expect("finite atom table record");
    assert_eq!(
        u16::from_le_bytes(atoms[..2].try_into().expect("atom count")),
        1,
        "atom table must contain the checked Ready identity"
    );
    let length = u16::from_le_bytes(atoms[2..4].try_into().expect("atom length")) as usize;
    assert_eq!(&atoms[4..4 + length], b"ready");
}
