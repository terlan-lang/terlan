use std::path::Path;

use super::support;

#[path = "direct_aot_cancellation.rs"]
mod cancellation;
#[path = "direct_aot_failure.rs"]
mod failure;
#[path = "direct_aot_link.rs"]
mod link;
#[path = "direct_aot_monitor.rs"]
mod monitor;
#[path = "direct_aot_receive.rs"]
mod receive;
#[path = "direct_aot_resource.rs"]
mod resource;
#[path = "direct_aot_scheduling.rs"]
mod scheduling;
#[path = "direct_aot_send.rs"]
mod send;
#[path = "direct_aot_spawn.rs"]
mod spawn;
#[path = "direct_aot_timer.rs"]
mod timer;

pub(super) struct NativeTransitionExports {
    pub(super) send: [u64; 2],
    pub(super) receive: [u64; 2],
    pub(super) spawn: [u64; 2],
    pub(super) timer: [u64; 2],
    pub(super) link: [u64; 2],
    pub(super) monitor: [u64; 2],
    pub(super) resource: [u64; 2],
    pub(super) cancellation: [u64; 2],
    pub(super) failure: [u64; 2],
    pub(super) scheduling: [u64; 2],
}

pub(super) fn assert_native_transition_suite(
    image_path: &Path,
    descriptor_digest: [u8; 32],
    exports: NativeTransitionExports,
) {
    send::assert_native_send_transitions(
        image_path,
        descriptor_digest,
        exports.send[0],
        exports.send[1],
    );
    receive::assert_native_receive_transitions(
        image_path,
        descriptor_digest,
        exports.receive[0],
        exports.receive[1],
    );
    spawn::assert_native_spawn_transitions(
        image_path,
        descriptor_digest,
        exports.spawn[0],
        exports.spawn[1],
    );
    spawn::assert_vm_spawn_then_send(image_path);
    timer::assert_native_timer_transitions(
        image_path,
        descriptor_digest,
        exports.timer[0],
        exports.timer[1],
    );
    timer::assert_vm_timer_transition(image_path);
    link::assert_native_link_transitions(
        image_path,
        descriptor_digest,
        exports.link[0],
        exports.link[1],
    );
    monitor::assert_native_monitor_transitions(
        image_path,
        descriptor_digest,
        exports.monitor[0],
        exports.monitor[1],
    );
    resource::assert_native_resource_transitions(
        image_path,
        descriptor_digest,
        exports.resource[0],
        exports.resource[1],
    );
    resource::assert_vm_resource_transition(image_path);
    cancellation::assert_native_cancellation_transitions(
        image_path,
        descriptor_digest,
        exports.cancellation[0],
        exports.cancellation[1],
    );
    failure::assert_native_failure_transitions(
        image_path,
        descriptor_digest,
        exports.failure[0],
        exports.failure[1],
    );
    failure::assert_vm_failure_transition(image_path);
    scheduling::assert_native_scheduling_transitions(
        image_path,
        descriptor_digest,
        exports.scheduling[0],
        exports.scheduling[1],
    );
    scheduling::assert_vm_scheduling_transition(image_path);
}
