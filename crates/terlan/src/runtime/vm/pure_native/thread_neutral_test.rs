use super::PureNativeSuspension;

/// Requires scheduler-migratable state at compile time.
fn assert_thread_neutral<T: Send + Sync + 'static>() {}

/// Prevents parked scheduler state from acquiring thread-bound fields.
#[test]
fn parked_native_continuation_is_send_sync_and_static() {
    assert_thread_neutral::<PureNativeSuspension>();
}
