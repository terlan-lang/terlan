use super::DriverReadinessPoller;

#[test]
fn serve_runtime_rejects_an_in_process_postgres_poller() {
    let error = DriverReadinessPoller::new().expect_err("the poller must be unavailable");
    assert_eq!(error.code(), "postgres.capability_worker.required");
    assert!(error.message().contains("capability-worker protocol"));
}
