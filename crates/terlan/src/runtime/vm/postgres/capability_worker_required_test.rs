use super::*;

#[test]
fn controls_fail_loudly_without_an_async_capability_worker() {
    let mut worker = VmPostgresLibpqWorker::default();
    let error = worker
        .apply_control(VmPostgresDriverControl::Cancel(RequestId { value: 1 }))
        .expect_err("in-process Postgres controls must be unavailable");
    assert_eq!(error.code, CAPABILITY_REQUIRED_CODE);
    assert!(error.message.contains("capability-worker protocol"));
}
