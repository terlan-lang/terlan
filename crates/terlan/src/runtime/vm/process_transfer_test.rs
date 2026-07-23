use super::*;

fn transfer_source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Transfer", name, 0)
}

#[test]
fn process_transfer_preserves_identity_names_stack_and_integrated_mailbox() {
    let mut source = VmProcessTable::default();
    let pid = source.spawn_root(transfer_source("run"));
    source.register_name("worker", pid).expect("register name");
    source
        .send(pid, pid, ReplValue::Int(73))
        .expect("publish self message");
    source
        .with_process_control_mutator(pid, |process| {
            process
                .enter_execution_frame(transfer_source("child"), 9, 17)
                .expect("enter frame");
        })
        .expect("mutate source process");

    let transfer = source
        .detach_process_for_transfer(pid)
        .expect("detach process");
    assert_eq!(transfer.process_id(), pid);
    assert_eq!(transfer.names(), &["worker"]);
    assert!(source.get(pid).is_none());
    assert!(source.lookup_name("worker").is_none());

    let mut destination = VmProcessTable::default();
    destination
        .import_process_transfer(transfer)
        .expect("import process");
    let snapshot = destination.snapshot(pid).expect("imported snapshot");
    assert_eq!(snapshot.registered_names, vec!["worker"]);
    assert_eq!(snapshot.mailbox_messages, 1);
    assert_eq!(snapshot.current_location.source.function, "child");
    let message = destination
        .with_process_control_mutator(pid, VmProcess::receive_next)
        .expect("receive imported message")
        .expect("message remains present");
    assert_eq!(message.payload, ReplValue::Int(73));

    let next = destination.spawn_root(transfer_source("next"));
    assert!(next.as_u64() > pid.as_u64());
}

#[test]
fn failed_process_import_returns_state_for_source_rollback() {
    let mut source = VmProcessTable::default();
    let pid = source.spawn_root(transfer_source("source"));
    source
        .register_name("source", pid)
        .expect("register source");
    let transfer = source
        .detach_process_for_transfer(pid)
        .expect("detach source");

    let mut destination = VmProcessTable::default();
    let collision = destination.spawn_root(transfer_source("collision"));
    assert_eq!(collision, pid);
    let failure = destination
        .import_process_transfer(transfer)
        .expect_err("identity collision must return process state");
    assert!(failure.reason().contains("already contains"));
    source
        .import_process_transfer(failure.into_transfer())
        .expect("restore source process");
    assert_eq!(source.lookup_name("source"), Some(pid));
    assert_eq!(
        source
            .snapshot(pid)
            .expect("restored source")
            .source
            .function,
        "source"
    );
}

#[test]
fn process_transfer_is_send_and_rejects_registered_name_collision() {
    fn assert_send<T: Send>() {}
    assert_send::<super::transfer::VmProcessTransfer>();

    let mut source = VmProcessTable::default();
    source.spawn_root(transfer_source("source-dummy"));
    let pid = source.spawn_root(transfer_source("source"));
    source
        .register_name("shared", pid)
        .expect("register source");
    let transfer = source
        .detach_process_for_transfer(pid)
        .expect("detach source");
    let mut destination = VmProcessTable::default();
    let owner = destination.spawn_root(transfer_source("owner"));
    destination
        .register_name("shared", owner)
        .expect("register destination name");
    let failure = destination
        .import_process_transfer(transfer)
        .expect_err("name collision must preserve transfer");
    assert!(failure.reason().contains("name `shared`"));
}
