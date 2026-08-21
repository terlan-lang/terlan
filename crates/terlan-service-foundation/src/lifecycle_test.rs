use super::*;

#[test]
fn readiness_stops_before_bounded_drain() {
    let mut lifecycle = Lifecycle::new(DrainBounds {
        max_in_flight: 64,
        max_actors: 128,
        max_native_resources: 32,
        max_flush_millis: 1_000,
    })
    .unwrap();
    lifecycle.mark_ready().unwrap();
    assert!(lifecycle.admits_requests());
    lifecycle.begin_drain().unwrap();
    assert!(!lifecycle.health().ready);
    assert!(!lifecycle.admits_requests());
    lifecycle
        .finish(DrainProgress {
            in_flight: 0,
            actors: 0,
            native_resources: 0,
            elapsed_millis: 100,
            telemetry_flushed: true,
        })
        .unwrap();
    assert_eq!(lifecycle.phase(), LifecyclePhase::Stopped);
}
