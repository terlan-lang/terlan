use super::{VmTableAccess, VmTableEntry, VmTableEvent, VmTableId, VmTableStore};
use crate::runtime::vm::process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessTable};
use crate::runtime::vm::ReplValue;

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

fn create_table(
    processes: &VmProcessTable,
    tables: &mut VmTableStore,
    owner: VmProcessId,
    name: &str,
    access: VmTableAccess,
) -> VmTableId {
    let event = tables
        .create(processes, owner, name, access)
        .expect("table creation should succeed");
    let id = tables
        .snapshots()
        .last()
        .expect("created table snapshot should exist")
        .id;
    assert_eq!(event, VmTableEvent::Created { id, owner });
    id
}

fn entry(key: &str, value: i64) -> VmTableEntry {
    VmTableEntry {
        key: ReplValue::String(key.to_string()),
        value: ReplValue::Int(value),
    }
}

#[test]
fn table_store_creates_owner_table_and_exposes_snapshot() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let mut tables = VmTableStore::default();

    let id = create_table(
        &processes,
        &mut tables,
        owner,
        "sessions",
        VmTableAccess::OwnerOnly,
    );

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
    let id = create_table(
        &processes,
        &mut tables,
        owner,
        "cache",
        VmTableAccess::OwnerOnly,
    );
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
fn table_store_traversal_preserves_stable_entry_order() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let mut tables = VmTableStore::default();
    let id = create_table(
        &processes,
        &mut tables,
        owner,
        "ordered",
        VmTableAccess::OwnerOnly,
    );

    for expected in [entry("first", 1), entry("second", 2), entry("third", 3)] {
        tables
            .insert(&processes, owner, id, expected.key.clone(), expected.value)
            .expect("ordered insert should succeed");
    }

    let first_key = ReplValue::String("first".to_string());
    let second_key = ReplValue::String("second".to_string());
    let third_key = ReplValue::String("third".to_string());
    assert_eq!(
        tables
            .first_entry(&processes, owner, id)
            .expect("first entry should be readable"),
        Some(entry("first", 1))
    );
    assert_eq!(
        tables
            .next_entry(&processes, owner, id, &first_key)
            .expect("next entry should be readable"),
        Some(entry("second", 2))
    );
    assert_eq!(
        tables
            .next_entry(&processes, owner, id, &second_key)
            .expect("next entry should be readable"),
        Some(entry("third", 3))
    );
    assert_eq!(
        tables
            .next_entry(&processes, owner, id, &third_key)
            .expect("last entry should terminate forward traversal"),
        None
    );
    assert_eq!(
        tables
            .last_entry(&processes, owner, id)
            .expect("last entry should be readable"),
        Some(entry("third", 3))
    );
    assert_eq!(
        tables
            .previous_entry(&processes, owner, id, &third_key)
            .expect("previous entry should be readable"),
        Some(entry("second", 2))
    );
    assert_eq!(
        tables
            .previous_entry(&processes, owner, id, &first_key)
            .expect("first entry should terminate reverse traversal"),
        None
    );
}

#[test]
fn table_store_traversal_handles_empty_replacement_deletion_and_missing_keys() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let mut tables = VmTableStore::default();
    let id = create_table(
        &processes,
        &mut tables,
        owner,
        "boundaries",
        VmTableAccess::OwnerOnly,
    );

    assert_eq!(
        tables
            .first_entry(&processes, owner, id)
            .expect("empty first entry should succeed"),
        None
    );
    assert_eq!(
        tables
            .last_entry(&processes, owner, id)
            .expect("empty last entry should succeed"),
        None
    );

    let first_key = ReplValue::String("first".to_string());
    let second_key = ReplValue::String("second".to_string());
    tables
        .insert(&processes, owner, id, first_key.clone(), ReplValue::Int(1))
        .expect("first insert should succeed");
    tables
        .insert(&processes, owner, id, first_key.clone(), ReplValue::Int(10))
        .expect("replacement should succeed");
    tables
        .insert(&processes, owner, id, second_key.clone(), ReplValue::Int(2))
        .expect("second insert should succeed");

    assert_eq!(
        tables
            .first_entry(&processes, owner, id)
            .expect("replacement should retain position"),
        Some(entry("first", 10))
    );
    tables
        .delete(&processes, owner, id, &first_key)
        .expect("first delete should succeed");
    assert_eq!(
        tables
            .first_entry(&processes, owner, id)
            .expect("deletion should advance first entry"),
        Some(entry("second", 2))
    );
    assert_eq!(
        tables
            .previous_entry(&processes, owner, id, &second_key)
            .expect("remaining entry should have no predecessor"),
        None
    );
    assert_eq!(
        tables
            .next_entry(&processes, owner, id, &first_key)
            .expect_err("deleted traversal key should fail"),
        format!("missing key in VM table {}", id.as_u64())
    );
}

#[test]
fn table_store_traversal_enforces_read_access() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let reader = processes.spawn_root(source("reader"));
    let mut tables = VmTableStore::default();
    let public = create_table(
        &processes,
        &mut tables,
        owner,
        "public",
        VmTableAccess::PublicRead,
    );
    let private = create_table(
        &processes,
        &mut tables,
        owner,
        "private",
        VmTableAccess::OwnerOnly,
    );
    tables
        .insert(
            &processes,
            owner,
            public,
            ReplValue::String("visible".to_string()),
            ReplValue::Int(1),
        )
        .expect("public table insert should succeed");

    assert_eq!(
        tables
            .first_entry(&processes, reader, public)
            .expect("public reader should traverse"),
        Some(entry("visible", 1))
    );
    assert_eq!(
        tables
            .first_entry(&processes, reader, private)
            .expect_err("private traversal should reject non-owner"),
        format!(
            "process {} cannot read table {} owned by process {}",
            reader.as_u64(),
            private.as_u64(),
            owner.as_u64()
        )
    );
}

#[test]
fn table_store_reports_missing_or_exited_processes_and_stale_handles() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let exited = processes.spawn_root(source("exited"));
    processes
        .exit_process(exited, VmExitReason::Normal)
        .expect("process should exit");
    let missing = VmProcessId::from_raw_for_test(99);
    let mut tables = VmTableStore::default();
    let id = create_table(
        &processes,
        &mut tables,
        owner,
        "cache",
        VmTableAccess::OwnerOnly,
    );
    let stale = VmTableId::from_raw_for_test(99);
    let key = ReplValue::String("answer".to_string());

    assert_eq!(
        tables
            .create(&processes, missing, "bad", VmTableAccess::OwnerOnly)
            .expect_err("missing owner should fail"),
        "missing owner process 99"
    );
    assert_eq!(
        tables
            .create(&processes, exited, "bad", VmTableAccess::OwnerOnly)
            .expect_err("exited owner should fail"),
        "owner process 2 has exited"
    );
    assert_eq!(
        tables
            .insert(&processes, missing, id, key.clone(), ReplValue::Int(1))
            .expect_err("missing requester insert should fail"),
        "missing requester process 99"
    );
    assert_eq!(
        tables
            .insert(&processes, exited, id, key.clone(), ReplValue::Int(1))
            .expect_err("exited requester insert should fail"),
        "requester process 2 has exited"
    );
    assert_eq!(
        tables
            .insert(&processes, owner, stale, key.clone(), ReplValue::Int(1))
            .expect_err("stale insert should fail"),
        "stale VM table handle 99"
    );
    assert_eq!(
        tables
            .lookup(&processes, missing, id, &key)
            .expect_err("missing requester lookup should fail"),
        "missing requester process 99"
    );
    assert_eq!(
        tables
            .lookup(&processes, exited, id, &key)
            .expect_err("exited requester lookup should fail"),
        "requester process 2 has exited"
    );
    assert_eq!(
        tables
            .lookup(&processes, owner, stale, &key)
            .expect_err("stale lookup should fail"),
        "stale VM table handle 99"
    );
    assert_eq!(
        tables
            .delete(&processes, missing, id, &key)
            .expect_err("missing requester delete should fail"),
        "missing requester process 99"
    );
    assert_eq!(
        tables
            .delete(&processes, exited, id, &key)
            .expect_err("exited requester delete should fail"),
        "requester process 2 has exited"
    );
    assert_eq!(
        tables
            .delete(&processes, owner, stale, &key)
            .expect_err("stale delete should fail"),
        "stale VM table handle 99"
    );
}

#[test]
fn table_store_delete_returns_none_for_missing_key() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let mut tables = VmTableStore::default();
    let id = create_table(
        &processes,
        &mut tables,
        owner,
        "cache",
        VmTableAccess::OwnerOnly,
    );

    assert_eq!(
        tables
            .delete(
                &processes,
                owner,
                id,
                &ReplValue::String("missing".to_string())
            )
            .expect("missing key delete should succeed"),
        None
    );
}

#[test]
fn table_store_public_read_allows_reads_but_rejects_non_owner_writes() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let reader = processes.spawn_root(source("reader"));
    let mut tables = VmTableStore::default();
    let id = create_table(
        &processes,
        &mut tables,
        owner,
        "catalog",
        VmTableAccess::PublicRead,
    );
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
    let id = create_table(
        &processes,
        &mut tables,
        owner,
        "private",
        VmTableAccess::OwnerOnly,
    );
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
    let id = create_table(
        &processes,
        &mut tables,
        owner,
        "shared",
        VmTableAccess::PublicReadWrite,
    );
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
fn table_store_public_read_write_allows_non_owner_delete() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let writer = processes.spawn_root(source("writer"));
    let mut tables = VmTableStore::default();
    let id = create_table(
        &processes,
        &mut tables,
        owner,
        "shared",
        VmTableAccess::PublicReadWrite,
    );
    let key = ReplValue::String("counter".to_string());
    tables
        .insert(&processes, owner, id, key.clone(), ReplValue::Int(1))
        .expect("owner insert should succeed");

    assert_eq!(
        tables
            .delete(&processes, writer, id, &key)
            .expect("public writer should delete"),
        Some(VmTableEvent::Deleted {
            id,
            key: key.clone(),
            old_value: ReplValue::Int(1)
        })
    );
    assert_eq!(
        tables
            .lookup(&processes, owner, id, &key)
            .expect("owner lookup should succeed"),
        None
    );
    assert_eq!(tables.snapshots()[0].len, 0);
}

#[test]
fn table_store_cleans_up_owner_tables_on_process_exit() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let other = processes.spawn_root(source("other"));
    let mut tables = VmTableStore::default();
    let owned_id = create_table(
        &processes,
        &mut tables,
        owner,
        "owned",
        VmTableAccess::OwnerOnly,
    );
    let other_id = create_table(
        &processes,
        &mut tables,
        other,
        "other",
        VmTableAccess::OwnerOnly,
    );

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
