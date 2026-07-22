use super::super::{
    code_server::{VmCodeServerEvent, VmModuleArtifact, VmModuleGenerationState},
    memory::VmMemoryLimits,
    process::{VmExitReason, VmProcessSource},
    ReplValue,
};
use super::VmActorRuntime;

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.ShardIsolation", name, 0)
}

fn runtime(node: &str) -> VmActorRuntime {
    VmActorRuntime::with_runtime_identity(VmMemoryLimits::new(1024, 4096).expect("limits"), node, 1)
        .expect("shard runtime")
}

#[test]
fn actor_runtime_services_and_image_generations_are_shard_local() {
    let mut first = runtime("shard-a");
    let mut second = runtime("shard-b");
    let first_owner = first.spawn_root(source("owner"));
    let first_peer = first.spawn_root(source("peer"));
    let second_owner = second.spawn_root(source("owner"));
    let second_peer = second.spawn_root(source("peer"));
    assert_eq!(first_owner, second_owner);
    assert_eq!(first_peer, second_peer);

    let first_timer = first
        .send_after(first_owner, first_peer, ReplValue::Int(11), 0, 10)
        .expect("first timer");
    let second_timer = second
        .send_after(second_owner, second_peer, ReplValue::Int(13), 0, 10)
        .expect("second timer");
    assert_eq!(first_timer, second_timer);

    first
        .park_native_continuation(first_owner.as_u64(), 17, 19)
        .expect("first resource continuation");
    second
        .park_native_continuation(second_owner.as_u64(), 17, 19)
        .expect("second resource continuation");
    assert_eq!(
        first
            .service_native_resource(first_owner.as_u64(), 17, 19, 23)
            .expect("first resource"),
        1
    );
    assert_eq!(
        second
            .service_native_resource(second_owner.as_u64(), 17, 19, 29)
            .expect("second resource"),
        1
    );

    first
        .link_actors(first_owner, first_peer)
        .expect("first link");
    let first_monitor = first
        .monitor_actor(first_owner, first_peer)
        .expect("first monitor");
    let second_monitor = second
        .monitor_actor(second_owner, second_peer)
        .expect("second monitor");
    assert_eq!(first_monitor.as_u64(), second_monitor.as_u64());
    assert_ne!(
        first_monitor.reference().node_id(),
        second_monitor.reference().node_id()
    );
    assert!(second
        .failure_snapshot(second_owner)
        .expect("second relationships")
        .links
        .is_empty());

    let first_generation = first.publish_image_generation(
        "app.Image",
        VmModuleArtifact::new("first-v1", "first-map-v1"),
    );
    let second_generation = second.publish_image_generation(
        "app.Image",
        VmModuleArtifact::new("second-v1", "second-map-v1"),
    );
    assert!(matches!(
        first_generation,
        VmCodeServerEvent::Published { .. }
    ));
    assert!(matches!(
        second_generation,
        VmCodeServerEvent::Published { .. }
    ));
    let (first_binding, first_retirement) = first
        .switch_actor_to_active_image(first_owner, "app.Image")
        .expect("first binding");
    let (second_binding, second_retirement) = second
        .switch_actor_to_active_image(second_owner, "app.Image")
        .expect("second binding");
    assert!(first_retirement.is_none());
    assert!(second_retirement.is_none());
    assert_eq!(first_binding.generation, second_binding.generation);

    first.publish_image_generation(
        "app.Image",
        VmModuleArtifact::new("first-v2", "first-map-v2"),
    );
    let first_images = first.image_generation_snapshots();
    let second_images = second.image_generation_snapshots();
    assert_eq!(first_images.len(), 2);
    assert_eq!(first_images[0].state, VmModuleGenerationState::Retiring);
    assert_eq!(first_images[1].state, VmModuleGenerationState::Active);
    assert_eq!(second_images.len(), 1);
    assert_eq!(second_images[0].checksum, "second-v1");

    first
        .exit_actor(first_owner, VmExitReason::Normal)
        .expect("first owner exit");
    assert!(first.resource_snapshots().is_empty());
    assert!(first.timers.snapshots().is_empty());
    assert_eq!(
        first.image_generation_snapshots()[0].state,
        VmModuleGenerationState::Retired
    );

    assert_eq!(second.resource_snapshots().len(), 1);
    assert_eq!(second.timers.snapshots().len(), 1);
    assert_eq!(
        second
            .failure_snapshot(second_owner)
            .expect("second monitor survives")
            .monitoring
            .len(),
        1
    );
    assert_eq!(second.image_generation_snapshots().len(), 1);
    assert_eq!(second.image_generation_snapshots()[0].active_processes, 1);
}
