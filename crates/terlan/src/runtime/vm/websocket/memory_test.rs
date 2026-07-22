use super::{
    VmAccountedWebSocketInboundQueue, VmAccountedWebSocketQueueError, VmWebSocketControlFrame,
    VmWebSocketFrame,
};
use crate::runtime::vm::{
    memory::{VmMemoryAccountant, VmMemoryLimits},
    process::{VmProcessSource, VmProcessTable},
    scheduler::VmScheduler,
};

fn owner(processes: &mut VmProcessTable) -> crate::runtime::vm::process::VmProcessId {
    processes.spawn_root(VmProcessSource::new("app.Http", "websocket", 0))
}

#[test]
fn vm_accounted_websocket_queue_rejects_pressure_before_frame_mutation() {
    let mut processes = VmProcessTable::default();
    let owner = owner(&mut processes);
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(4, 5).expect("limits"));
    let mut scheduler = VmScheduler::default();
    let mut queue = VmAccountedWebSocketInboundQueue::new(owner, 4, 16);

    queue
        .push(
            &mut memory,
            &mut scheduler,
            &mut processes,
            VmWebSocketFrame::Text("one".to_string()),
        )
        .expect("first frame");
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 3);
    assert_eq!(
        queue
            .push(
                &mut memory,
                &mut scheduler,
                &mut processes,
                VmWebSocketFrame::Text("two".to_string()),
            )
            .expect_err("second frame exceeds hard limit"),
        VmAccountedWebSocketQueueError::MemoryPressureRejected
    );
    assert_eq!(queue.inspect().pending_frames, 1);
    assert_eq!(queue.inspect().queued_frame_bytes, 3);
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 3);
    assert_eq!(
        queue
            .pop(&mut memory, &mut scheduler, &mut processes)
            .expect("pop"),
        Some(VmWebSocketFrame::Text("one".to_string()))
    );
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 0);
    assert_eq!(scheduler.memory_reductions(owner), 6);
    assert_eq!(scheduler.total_memory_reductions(), 6);
}

#[test]
fn vm_accounted_websocket_queue_cancellation_releases_pending_frames() {
    let mut processes = VmProcessTable::default();
    let owner = owner(&mut processes);
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(64, 128).expect("limits"));
    let mut scheduler = VmScheduler::default();
    let mut queue = VmAccountedWebSocketInboundQueue::new(owner, 4, 16);

    queue
        .push(
            &mut memory,
            &mut scheduler,
            &mut processes,
            VmWebSocketFrame::Text("one".to_string()),
        )
        .expect("text frame");
    queue
        .push(
            &mut memory,
            &mut scheduler,
            &mut processes,
            VmWebSocketFrame::Control(VmWebSocketControlFrame::Ping(b"two".to_vec())),
        )
        .expect("ping frame");
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 6);
    assert_eq!(
        queue
            .cancel(&mut memory, &mut scheduler, &mut processes)
            .expect("cancel queue"),
        2
    );
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 0);
    assert_eq!(queue.inspect().pending_frames, 0);
    assert_eq!(queue.inspect().queued_frame_bytes, 0);
    assert_eq!(scheduler.memory_reductions(owner), 6);
    assert_eq!(scheduler.total_memory_reductions(), 6);
    assert_eq!(
        queue
            .push(
                &mut memory,
                &mut scheduler,
                &mut processes,
                VmWebSocketFrame::Text("late".to_string()),
            )
            .expect_err("cancelled queue rejects frames"),
        VmAccountedWebSocketQueueError::Queue(
            "error[vm_websocket_queue]: queue is cancelled".to_string()
        )
    );
}
