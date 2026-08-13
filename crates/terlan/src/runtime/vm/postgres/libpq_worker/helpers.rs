use super::*;

pub(super) fn sql_pending(
    request_id: RequestId,
    connection: DriverConnection,
    return_connection: ReturnConnection,
    task: PendingTask,
    sql: String,
) -> PendingIo {
    PendingIo {
        request_id,
        connection: Some(connection),
        return_connection,
        task,
        phase: PendingPhase::Sending {
            sql,
            parameters: Vec::new(),
        },
    }
}

pub(super) fn stale(kind: &str) -> postgres::PostgresError {
    postgres::PostgresError::new(
        "postgres.driver.stale_resource",
        format!("Postgres driver {kind} resource is not live."),
    )
}

pub(super) fn stale_failure(kind: &str) -> VmPostgresFailure {
    VmPostgresFailure::new(
        "postgres.driver.stale_resource",
        format!("Postgres driver {kind} resource is not live."),
    )
}

pub(super) fn cancelled() -> VmPostgresDriverCompletion {
    VmPostgresDriverCompletion::Failed(VmPostgresFailure::new(
        "postgres.cancelled",
        "Postgres request was cancelled.",
    ))
}

pub(super) fn failure(error: postgres::PostgresError) -> VmPostgresDriverCompletion {
    VmPostgresDriverCompletion::Failed(adapter_failure(error))
}

pub(super) fn failure_for_owner(
    error: postgres::PostgresError,
    owner: crate::runtime::vm::process::VmProcessId,
) -> VmPostgresDriverCompletion {
    VmPostgresDriverCompletion::Failed(VmPostgresFailure::new(
        error.code(),
        format!(
            "Postgres request owned by process {} failed: {}",
            owner.as_u64(),
            error.message()
        ),
    ))
}

pub(super) fn adapter_failure(error: postgres::PostgresError) -> VmPostgresFailure {
    VmPostgresFailure::new(error.code(), error.message())
}
