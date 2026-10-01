use super::*;
use std::sync::Mutex;
use terlan_runtime_abi::{NativeContextBinding, NativeValue};

#[test]
fn native_service_grants_follow_image_forks_not_actor_heap_lifetimes() {
    let mut template = ManagedExecutionRuntime::runtime_default().unwrap();
    assert!(template.fork_empty().native_services.is_none());
    let context = Arc::new(Mutex::new(0_i64));
    let weak = Arc::downgrade(&context);
    let mut grants = NativeServices::default();
    grants
        .register_context(
            context,
            [NativeContextBinding::new(
                "example.counter.next",
                0,
                |count: &mut i64, _| {
                    *count += 1;
                    Ok(NativeValue::Int(*count))
                },
            )],
        )
        .unwrap();
    template.attach_native_services(grants);
    let mut first = template.fork_empty();
    let second = template.fork_empty();
    drop(template);
    first
        .heap(1)
        .unwrap()
        .allocate_string("private actor value")
        .unwrap();
    first.release_owner(1);
    assert_eq!(
        first
            .native_services
            .as_ref()
            .unwrap()
            .call("example.counter.next", &[]),
        Ok(1.into())
    );
    assert_eq!(
        second
            .native_services
            .as_ref()
            .unwrap()
            .call("example.counter.next", &[]),
        Ok(2.into())
    );
    drop(first);
    assert!(weak.upgrade().is_some());
    drop(second);
    assert!(weak.upgrade().is_none());
}
