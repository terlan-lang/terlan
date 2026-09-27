use super::*;

#[test]
fn invalid_url_is_redacted_and_returns_request_credit() {
    let mut executor = PostgresExecutor::default();
    let mut worker = NativeBoundaryWorker::new(1);
    let call = || CapabilityCall {
        request_id: 1,
        owner_id: 7,
        capability: "postgres".into(),
        operation: "runtime.postgres.connect".into(),
        arguments: vec![CapabilityValue::Text(
            "https://private:secret@localhost/db".into(),
        )],
    };
    let reply = executor.call(&mut worker, call(), &NativeBoundaryCancellationToken::new());
    match reply.result {
        NativeBoundaryReplyTerm::Error { code, message, .. } => {
            assert_eq!(code, "postgres.invalid_url");
            assert!(!message.contains("secret"));
        }
        other => panic!("unexpected reply: {other:?}"),
    }
    assert_eq!(reply.available_credits, 1);
    assert_eq!(reply.reserved_credits, 0);
    assert!(matches!(
        executor
            .call(&mut worker, call(), &NativeBoundaryCancellationToken::new())
            .result,
        NativeBoundaryReplyTerm::Error { .. }
    ));
}
#[test]
fn cancellation_precedes_database_execution_and_releases_credit() {
    let mut executor = PostgresExecutor::default();
    let mut worker = NativeBoundaryWorker::new(1);
    let cancellation = NativeBoundaryCancellationToken::new();
    cancellation.cancel();
    let reply = executor.call(
        &mut worker,
        CapabilityCall {
            request_id: 1,
            owner_id: 7,
            capability: "postgres".into(),
            operation: "runtime.postgres.connect".into(),
            arguments: vec![CapabilityValue::Text("postgres://localhost/test".into())],
        },
        &cancellation,
    );
    assert!(matches!(
        reply.result,
        NativeBoundaryReplyTerm::Error { .. }
    ));
    assert_eq!(reply.available_credits, 1);
    assert_eq!(reply.reserved_credits, 0);
}
#[cfg(all(feature = "postgres-libpq", not(feature = "serve-runtime-bin")))]
#[test]
fn worker_pool_handles_check_owner_generation_and_kind_before_io() {
    let mut executor = PostgresExecutor::default();
    let pool = executor
        .execute(
            7,
            "runtime.postgres.connect",
            &[CapabilityValue::Text("postgres://localhost/test".into())],
        )
        .unwrap();
    let args = |pool| {
        vec![
            pool,
            CapabilityValue::Text(" ".into()),
            CapabilityValue::List(vec![]),
        ]
    };
    assert!(executor
        .execute(8, "runtime.postgres.query", &args(pool.clone()))
        .is_err());
    assert!(executor
        .execute(
            7,
            "runtime.postgres.string",
            &[pool.clone(), CapabilityValue::Text("column".into())]
        )
        .is_err());
    let CapabilityValue::Handle(mut stale) = pool.clone() else {
        panic!("pool handle");
    };
    stale.generation += 1;
    assert!(executor
        .execute(
            7,
            "runtime.postgres.execute",
            &args(CapabilityValue::Handle(stale))
        )
        .is_err());
    assert!(
        matches!(executor.execute(7, "runtime.postgres.execute", &args(pool)), Err(NativeBoundaryReplyTerm::Error { code, .. }) if code == "postgres.sql.empty")
    );
}
#[cfg(any(not(feature = "postgres-libpq"), feature = "serve-runtime-bin"))]
#[test]
fn compiler_free_worker_returns_explicit_driver_unavailable() {
    let mut executor = PostgresExecutor::default();
    assert!(
        matches!(executor.execute(7, "runtime.postgres.connect", &[CapabilityValue::Text("postgres://localhost/test".into())]), Err(NativeBoundaryReplyTerm::Error { code, .. }) if code == "postgres.driver_unavailable")
    );
}
