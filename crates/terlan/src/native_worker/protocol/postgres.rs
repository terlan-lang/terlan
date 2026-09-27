//! Database execution confined to the external capability worker.

use super::execution::CapabilityCall;
use crate::terlan_native_boundary::{
    cancellation::NativeBoundaryCancellationToken,
    capability_wire::CapabilityValue,
    request::RequestId,
    term::NativeBoundaryReplyTerm,
    worker::{NativeBoundaryWorker, NativeBoundaryWorkerReply},
};

#[cfg(all(feature = "postgres-libpq", not(feature = "serve-runtime-bin")))]
mod driver;

/// The internal wire accepts JSON text, not handles owned by another process.
pub(super) fn admits(operation: &str) -> bool {
    matches!(
        operation,
        "runtime.postgres.connect"
            | "runtime.postgres.query"
            | "runtime.postgres.query_one"
            | "runtime.postgres.execute"
            | "runtime.postgres.begin"
            | "runtime.postgres.commit"
            | "runtime.postgres.rollback"
            | "runtime.postgres.string"
            | "runtime.postgres.int"
            | "runtime.postgres.bool"
            | "runtime.postgres.json"
    )
}

type Result<T> = std::result::Result<T, NativeBoundaryReplyTerm>;

#[derive(Default)]
pub(super) struct PostgresExecutor {
    #[cfg(all(feature = "postgres-libpq", not(feature = "serve-runtime-bin")))]
    driver: driver::Driver,
}

impl PostgresExecutor {
    pub(super) fn call(
        &mut self,
        worker: &mut NativeBoundaryWorker,
        call: CapabilityCall,
        cancellation: &NativeBoundaryCancellationToken,
    ) -> NativeBoundaryWorkerReply {
        let id = RequestId {
            value: call.request_id,
        };
        let result = match worker.begin_request(id) {
            Err(error) => error,
            Ok(()) => {
                let reply = if cancellation.is_cancelled() {
                    Err(error(
                        "postgres.cancelled",
                        "cancelled before database execution",
                    ))
                } else {
                    self.execute(call.owner_id, &call.operation, &call.arguments)
                };
                if cancellation.is_cancelled() {
                    worker.cancel_request(id)
                } else {
                    match worker.finish_request(id) {
                        Err(error) => error,
                        Ok(()) => reply.map_or_else(
                            |error| error,
                            |value| NativeBoundaryReplyTerm::Ok(value.into_term()),
                        ),
                    }
                }
            }
        };
        NativeBoundaryWorkerReply {
            request_id: id,
            result,
            reserved_credits: worker.reserved_credits(),
            available_credits: worker.available_credits(),
        }
    }

    fn execute(
        &mut self,
        owner: u64,
        operation: &str,
        args: &[CapabilityValue],
    ) -> Result<CapabilityValue> {
        // Validate configurations even in executables deliberately built without libpq.
        if operation == "runtime.postgres.connect" {
            let [CapabilityValue::Text(url)] = args else {
                return Err(shape_error());
            };
            crate::terlan_native::postgres::validate_config(
                &crate::terlan_native::postgres::Config::new(url),
            )
            .map_err(|failure| error(failure.code(), failure.message()))?;
        }
        #[cfg(all(feature = "postgres-libpq", not(feature = "serve-runtime-bin")))]
        {
            self.driver.execute(owner, operation, args)
        }
        #[cfg(not(all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))))]
        {
            let _ = owner;
            Err(error(
                "postgres.driver_unavailable",
                "this worker was built without the Postgres driver",
            ))
        }
    }
}

fn error(code: &str, message: &str) -> NativeBoundaryReplyTerm {
    NativeBoundaryReplyTerm::Error {
        code: code.into(),
        message: message.into(),
        offset: 0,
    }
}
fn shape_error() -> NativeBoundaryReplyTerm {
    error(
        "postgres.arguments",
        "invalid database operation or argument shape",
    )
}

#[cfg(test)]
#[path = "postgres/postgres_test.rs"]
mod tests;
