use super::{VmTableAccess, VmTableEvent, VmTableStore};
use crate::runtime::vm::process::{VmExitReason, VmProcessSource, VmProcessTable};
use crate::runtime::vm::ReplValue;

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

#[test]
fn table_store_creates_owner_table_and_exposes_snapshot() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let mut tables = VmTableStore::default();

    let event = tables
        .create(&processes, owner, "sessions", VmTableAccess::OwnerOnly)
        .expect("table creation should succeed");

    let VmTableEvent::Created {
        id,
        owner: table_owner,
    } = event
    else {
        panic!("expected table creation event");
    };
    assert_eq!(table_owner, owner);

    let snapshots = tables.snapshots();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].id, id);
    assert_eq!(snapshots[0].owner, owner);
    assert_eq!(snapshots[0].name, "sessions");
    assert_eq!(snapshots[0].access, VmTableAccess::OwnerOnly);
    assert_eq!(snapshots[0].len, 0);
}

#[test]
fn table_store_inserts_replaces_looks_up_and_deletes_values() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let mut tables = VmTableStore::default();
    let VmTableEvent::Created { id, .. } = tables
        .create(&processes, owner, "cache", VmTableAccess::OwnerOnly)
        .expect("table creation should succeed")
    else {
        panic!("expected table creation event");
    };
    let key = ReplValue::String("answer".to_string());

    assert_eq!(
        tables
            .insert(&processes, owner, id, key.clone(), ReplValue::Int(41))
            .expect("insert should succeed"),
        VmTableEvent::Inserted {
            id,
            key: key.clone()
        }
    );
    assert_eq!(
        tables
            .insert(&processes, owner, id, key.clone(), ReplValue::Int(42))
            .expect("replace should succeed"),
        VmTableEvent::Replaced {
            id,
            key: key.clone(),
            old_value: ReplValue::Int(41)
        }
    );
    assert_eq!(
        tables
            .lookup(&processes, owner, id, &key)
            .expect("lookup should succeed"),
        Some(ReplValue::Int(42))
    );
    assert_eq!(
        tables
            .delete(&processes, owner, id, &key)
            .expect("delete should succeed"),
        Some(VmTableEvent::Deleted {
            id,
            key: key.clone(),
            old_value: ReplValue::Int(42)
        })
    );
    assert_eq!(
        tables
            .lookup(&processes, owner, id, &key)
            .expect("lookup after delete should succeed"),
        None
    );
}

#[test]
fn table_store_public_read_allows_reads_but_rejects_non_owner_writes() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let reader = processes.spawn_root(source("reader"));
    let mut tables = VmTableStore::default();
    let VmTableEvent::Created { id, .. } = tables
        .create(&processes, owner, "catalog", VmTableAccess::PublicRead)
        .expect("table creation should succeed")
    else {
        panic!("expected table creation event");
    };
    let key = ReplValue::String("name".to_string());
    tables
        .insert(
            &processes,
            owner,
            id,
            key.clone(),
            ReplValue::String("Terlan".to_string()),
        )
        .expect("owner insert should succeed");

    assert_eq!(
        tables
            .lookup(&processes, reader, id, &key)
            .expect("public read should succeed"),
        Some(ReplValue::String("Terlan".to_string()))
    );
    assert_eq!(
        tables
            .insert(
                &processes,
                reader,
                id,
                key.clone(),
                ReplValue::String("x".to_string())
            )
            .expect_err("public reader should not write"),
        format!(
            "process {} cannot write table {} owned by process {}",
            reader.as_u64(),
            id.as_u64(),
            owner.as_u64()
        )
    );
}

#[test]
fn table_store_owner_only_rejects_non_owner_reads_and_writes() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let other = processes.spawn_root(source("other"));
    let mut tables = VmTableStore::default();
    let VmTableEvent::Created { id, .. } = tables
        .create(&processes, owner, "private", VmTableAccess::OwnerOnly)
        .expect("table creation should succeed")
    else {
        panic!("expected table creation event");
    };
    let key = ReplValue::String("token".to_string());

    assert_eq!(
        tables
            .lookup(&processes, other, id, &key)
            .expect_err("non-owner read should fail"),
        format!(
            "process {} cannot read table {} owned by process {}",
            other.as_u64(),
            id.as_u64(),
            owner.as_u64()
        )
    );
    assert_eq!(
        tables
            .delete(&processes, other, id, &key)
            .expect_err("non-owner write should fail"),
        format!(
            "process {} cannot write table {} owned by process {}",
            other.as_u64(),
            id.as_u64(),
            owner.as_u64()
        )
    );
}

#[test]
fn table_store_public_read_write_allows_non_owner_mutation() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let writer = processes.spawn_root(source("writer"));
    let mut tables = VmTableStore::default();
    let VmTableEvent::Created { id, .. } = tables
        .create(&processes, owner, "shared", VmTableAccess::PublicReadWrite)
        .expect("table creation should succeed")
    else {
        panic!("expected table creation event");
    };
    let key = ReplValue::String("counter".to_string());

    tables
        .insert(&processes, writer, id, key.clone(), ReplValue::Int(1))
        .expect("public writer should insert");

    assert_eq!(
        tables
            .lookup(&processes, owner, id, &key)
            .expect("owner read should succeed"),
        Some(ReplValue::Int(1))
    );
}

#[test]
fn table_store_cleans_up_owner_tables_on_process_exit() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let other = processes.spawn_root(source("other"));
    let mut tables = VmTableStore::default();
    let VmTableEvent::Created { id: owned_id, .. } = tables
        .create(&processes, owner, "owned", VmTableAccess::OwnerOnly)
        .expect("table creation should succeed")
    else {
        panic!("expected table creation event");
    };
    let VmTableEvent::Created { id: other_id, .. } = tables
        .create(&processes, other, "other", VmTableAccess::OwnerOnly)
        .expect("table creation should succeed")
    else {
        panic!("expected table creation event");
    };

    processes
        .exit_process(owner, VmExitReason::Normal)
        .expect("owner exit should succeed");
    assert_eq!(
        tables.cleanup_owner(owner),
        vec![VmTableEvent::CleanedUpOnExit {
            id: owned_id,
            owner
        }]
    );
    assert_eq!(
        tables
            .lookup(
                &processes,
                other,
                owned_id,
                &ReplValue::String("x".to_string())
            )
            .expect_err("cleaned table should be stale"),
        format!("stale VM table handle {}", owned_id.as_u64())
    );
    assert_eq!(tables.snapshots().len(), 1);
    assert_eq!(tables.snapshots()[0].id, other_id);
}
