use super::types::{
    VmPostgresDriverCompletion, VmPostgresDriverOperation, VmPostgresFailure, VmPostgresReply,
};
use crate::runtime::vm::native_boundary::deadline::VmNativeBoundaryDeadlineCompletion;
use crate::terlan_native_boundary::{request::RequestId, term::NativeBoundaryReplyTerm};

pub(super) fn validate_sql(sql: String) -> Result<String, String> {
    if sql.trim().is_empty() {
        Err("error[postgres.sql.empty]: Postgres SQL text must not be empty".to_string())
    } else {
        Ok(sql)
    }
}

pub(super) fn validate_driver_completion(
    operation: &VmPostgresDriverOperation,
    completion: &VmPostgresDriverCompletion,
) -> Result<(), String> {
    let valid = matches!(completion, VmPostgresDriverCompletion::Failed(_));
    #[cfg(any(
        test,
        all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
    ))]
    let valid = valid
        || matches!(
            (operation, completion),
            (
                VmPostgresDriverOperation::Connect(_),
                VmPostgresDriverCompletion::Connected(_)
            ) | (
                VmPostgresDriverOperation::Acquire { .. },
                VmPostgresDriverCompletion::Acquired(_)
            ) | (
                VmPostgresDriverOperation::Begin { .. },
                VmPostgresDriverCompletion::TransactionStarted(_)
            ) | (
                VmPostgresDriverOperation::Commit { .. }
                    | VmPostgresDriverOperation::Rollback { .. },
                VmPostgresDriverCompletion::Unit
            ) | (
                VmPostgresDriverOperation::Query { .. },
                VmPostgresDriverCompletion::Rows { .. }
            ) | (
                VmPostgresDriverOperation::Execute { .. },
                VmPostgresDriverCompletion::AffectedRows(_)
            ) | (
                VmPostgresDriverOperation::BatchExecute { .. },
                VmPostgresDriverCompletion::Unit
            ) | (
                VmPostgresDriverOperation::Decode { .. },
                VmPostgresDriverCompletion::Decoded(_)
            )
        );
    #[cfg(test)]
    let valid = valid
        || matches!(
            (operation, completion),
            (
                VmPostgresDriverOperation::Prepare { .. },
                VmPostgresDriverCompletion::Prepared(_)
            )
        );
    if valid {
        Ok(())
    } else {
        Err(format!(
            "error[postgres.driver.protocol]: invalid completion for {}",
            operation.name()
        ))
    }
}

pub(super) fn deadline_reply(
    completion: VmNativeBoundaryDeadlineCompletion,
) -> (RequestId, &'static str, NativeBoundaryReplyTerm) {
    match completion {
        VmNativeBoundaryDeadlineCompletion::TimedOut {
            request_id, reply, ..
        } => (request_id, "timed_out", reply),
        VmNativeBoundaryDeadlineCompletion::Cancelled {
            request_id, reply, ..
        } => (request_id, "cancelled", reply),
        VmNativeBoundaryDeadlineCompletion::OwnerExited {
            request_id, reply, ..
        } => (request_id, "owner_exited", reply),
        VmNativeBoundaryDeadlineCompletion::Completed { .. } => {
            unreachable!("driver completion is handled by VmPostgresRuntime::complete")
        }
    }
}

pub(super) fn worker_reply_to_postgres(
    reply: NativeBoundaryReplyTerm,
    outcome: &str,
) -> VmPostgresReply {
    match reply {
        NativeBoundaryReplyTerm::Error { code, message, .. } => {
            VmPostgresReply::Error(VmPostgresFailure::new(
                format!("postgres.{outcome}"),
                format!("Postgres operation ended before completion ({code}): {message}"),
            ))
        }
        NativeBoundaryReplyTerm::Ok(_) => VmPostgresReply::Error(VmPostgresFailure::new(
            "postgres.lifecycle",
            "Postgres lifecycle ended without a terminal error.",
        )),
    }
}
