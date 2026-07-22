use std::{collections::BTreeSet, time::Duration};

use crate::terlan_native::postgres::libpq::{
    DriverIoInterest, DriverReadinessPoller, DriverReadinessSource,
};

use super::*;

impl VmPostgresLibpqWorker {
    pub(crate) fn wait_ready(
        &self,
        poller: &DriverReadinessPoller,
        timeout: Option<Duration>,
    ) -> Result<BTreeSet<u64>, postgres::PostgresError> {
        let sources = self
            .waits
            .values()
            .map(|wait| {
                let pending = self.active.get(&wait.request_id.value).ok_or_else(|| {
                    postgres::PostgresError::new(
                        "postgres.readiness.missing_request",
                        format!(
                            "Postgres readiness request {} is not active.",
                            wait.request_id.value
                        ),
                    )
                })?;
                let connection = pending.connection.as_ref().ok_or_else(|| {
                    postgres::PostgresError::new(
                        "postgres.readiness.missing_connection",
                        format!(
                            "Postgres readiness request {} does not own a connection.",
                            wait.request_id.value
                        ),
                    )
                })?;
                Ok(DriverReadinessSource {
                    key: wait.request_id.value,
                    connection,
                    interest: match wait.interest {
                        VmPostgresIoInterest::Drive => DriverIoInterest::Drive,
                        VmPostgresIoInterest::Read => DriverIoInterest::Read,
                        VmPostgresIoInterest::Write => DriverIoInterest::Write,
                    },
                })
            })
            .collect::<Result<Vec<_>, postgres::PostgresError>>()?;
        poller
            .wait(&sources, timeout)
            .map(|ready| ready.into_iter().collect())
    }
}
