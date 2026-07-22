use std::io::{self, Write};

use super::template_response::{VmAccountedHttpOutputError, VmAccountedHttpTemplateResponse};
use crate::runtime::vm::{
    memory::{VmMemoryAccountant, VmMemoryLimits, VmSharedAllocationKind},
    process::{VmProcessSource, VmProcessTable},
    scheduler::VmScheduler,
};

fn owner(processes: &mut VmProcessTable) -> crate::runtime::vm::process::VmProcessId {
    processes.spawn_root(VmProcessSource::new("app.Http", "template", 0))
}

#[test]
fn accounted_template_output_promotes_to_response_and_releases_after_write() {
    let mut processes = VmProcessTable::default();
    let owner = owner(&mut processes);
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(32, 64).expect("limits"));
    let mut scheduler = VmScheduler::default();
    let template = VmAccountedHttpTemplateResponse::html(
        &mut memory,
        &mut scheduler,
        &mut processes,
        owner,
        "UserCard",
        "templates/user_card.terl.html",
        "<article>Ada</article>",
    )
    .expect("accounted template");
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 22);

    let response = template
        .render(&mut memory, http::StatusCode::OK)
        .expect("response");
    let allocation = response.allocation();
    assert_eq!(
        memory.shared_allocation_kind(allocation),
        Some(VmSharedAllocationKind::ResponseBuffer)
    );
    let mut wire = Vec::new();
    response
        .write(
            &mut memory,
            &mut scheduler,
            &mut processes,
            &mut wire,
            false,
        )
        .expect("write");
    assert!(String::from_utf8(wire)
        .expect("utf8")
        .ends_with("<article>Ada</article>"));
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 0);
    assert_eq!(memory.shared_allocation_kind(allocation), None);
    assert_eq!(scheduler.memory_reductions(owner), 4);
}

#[test]
fn accounted_template_output_rejects_hard_pressure_before_ownership() {
    let mut processes = VmProcessTable::default();
    let owner = owner(&mut processes);
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(4, 8).expect("limits"));
    let mut scheduler = VmScheduler::default();

    assert_eq!(
        VmAccountedHttpTemplateResponse::html(
            &mut memory,
            &mut scheduler,
            &mut processes,
            owner,
            "Large",
            "templates/large.terl.html",
            "123456789",
        )
        .expect_err("hard pressure"),
        VmAccountedHttpOutputError::MemoryPressureRejected
    );
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 0);
    assert_eq!(scheduler.memory_reductions(owner), 2);
}

#[test]
fn accounted_template_output_cancellation_releases_ownership() {
    let mut processes = VmProcessTable::default();
    let owner = owner(&mut processes);
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(16, 32).expect("limits"));
    let mut scheduler = VmScheduler::default();
    let template = VmAccountedHttpTemplateResponse::html(
        &mut memory,
        &mut scheduler,
        &mut processes,
        owner,
        "Cancelled",
        "templates/cancelled.terl.html",
        "stop",
    )
    .expect("template");
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 4);

    template
        .cancel(&mut memory, &mut scheduler, &mut processes)
        .expect("cancel");
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 0);
    assert_eq!(scheduler.memory_reductions(owner), 4);
}

#[test]
fn accounted_response_releases_ownership_when_writer_fails() {
    let mut processes = VmProcessTable::default();
    let owner = owner(&mut processes);
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(16, 32).expect("limits"));
    let mut scheduler = VmScheduler::default();
    let template = VmAccountedHttpTemplateResponse::html(
        &mut memory,
        &mut scheduler,
        &mut processes,
        owner,
        "Error",
        "templates/error.terl.html",
        "fail",
    )
    .expect("template");
    let response = template
        .render(&mut memory, http::StatusCode::INTERNAL_SERVER_ERROR)
        .expect("response");
    let allocation = response.allocation();
    let mut writer = RejectWriter;

    assert!(matches!(
        response.write(
            &mut memory,
            &mut scheduler,
            &mut processes,
            &mut writer,
            true,
        ),
        Err(VmAccountedHttpOutputError::Write(_))
    ));
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 0);
    assert_eq!(memory.shared_allocation_kind(allocation), None);
    assert_eq!(scheduler.memory_reductions(owner), 4);
}

struct RejectWriter;

impl Write for RejectWriter {
    fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("injected writer failure"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
