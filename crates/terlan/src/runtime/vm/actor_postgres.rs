use std::path::Path;

use super::{VmActorRuntime, VmProcessId};
use crate::{
    runtime::vm::{
        postgres::{
            VmPostgresConnectConfig, VmPostgresConnection, VmPostgresDecodeType,
            VmPostgresDriverCompletion, VmPostgresDriverControl, VmPostgresDriverRequest,
            VmPostgresPool, VmPostgresQueryTarget, VmPostgresReply, VmPostgresRow,
            VmPostgresTransaction,
        },
        timer::VmTimerEvent,
    },
    terlan_native::json,
    terlan_native_boundary::request::RequestId,
};

impl VmActorRuntime {
    pub(crate) fn postgres_connect(
        &mut self,
        owner: VmProcessId,
        config: VmPostgresConnectConfig,
        now_tick: u64,
        timeout_ticks: u64,
    ) -> Result<RequestId, String> {
        self.postgres.connect(
            &mut self.timers,
            &mut self.processes,
            &mut self.scheduler,
            owner,
            config,
            now_tick,
            timeout_ticks,
        )
    }

    pub(crate) fn postgres_acquire(
        &mut self,
        owner: VmProcessId,
        pool: VmPostgresPool,
        now_tick: u64,
        timeout_ticks: u64,
    ) -> Result<RequestId, String> {
        self.postgres.acquire(
            &mut self.timers,
            &mut self.processes,
            &mut self.scheduler,
            owner,
            pool,
            now_tick,
            timeout_ticks,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn postgres_query(
        &mut self,
        owner: VmProcessId,
        target: VmPostgresQueryTarget,
        sql: impl Into<String>,
        parameters: Vec<json::Json>,
        one: bool,
        now_tick: u64,
        timeout_ticks: u64,
    ) -> Result<RequestId, String> {
        self.postgres.query(
            &mut self.timers,
            &mut self.processes,
            &mut self.scheduler,
            owner,
            target,
            sql,
            parameters,
            one,
            now_tick,
            timeout_ticks,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn postgres_execute(
        &mut self,
        owner: VmProcessId,
        target: VmPostgresQueryTarget,
        sql: impl Into<String>,
        parameters: Vec<json::Json>,
        now_tick: u64,
        timeout_ticks: u64,
    ) -> Result<RequestId, String> {
        self.postgres.execute(
            &mut self.timers,
            &mut self.processes,
            &mut self.scheduler,
            owner,
            target,
            sql,
            parameters,
            now_tick,
            timeout_ticks,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn postgres_batch_execute(
        &mut self,
        owner: VmProcessId,
        target: VmPostgresQueryTarget,
        sql: impl Into<String>,
        now_tick: u64,
        timeout_ticks: u64,
    ) -> Result<RequestId, String> {
        self.postgres.batch_execute(
            &mut self.timers,
            &mut self.processes,
            &mut self.scheduler,
            owner,
            target,
            sql,
            now_tick,
            timeout_ticks,
        )
    }

    pub(crate) fn postgres_begin(
        &mut self,
        owner: VmProcessId,
        connection: VmPostgresConnection,
        now_tick: u64,
        timeout_ticks: u64,
    ) -> Result<RequestId, String> {
        self.postgres.begin(
            &mut self.timers,
            &mut self.processes,
            &mut self.scheduler,
            owner,
            connection,
            now_tick,
            timeout_ticks,
        )
    }

    pub(crate) fn postgres_release_connection(
        &mut self,
        owner: VmProcessId,
        connection: VmPostgresConnection,
    ) -> Result<(), String> {
        let control = self.postgres.release_connection(owner, connection)?;
        self.postgres_controls.push_back(control);
        Ok(())
    }

    pub(crate) fn postgres_finish_transaction(
        &mut self,
        owner: VmProcessId,
        transaction: VmPostgresTransaction,
        commit: bool,
        now_tick: u64,
        timeout_ticks: u64,
    ) -> Result<RequestId, String> {
        self.postgres.finish_transaction(
            &mut self.timers,
            &mut self.processes,
            &mut self.scheduler,
            owner,
            transaction,
            commit,
            now_tick,
            timeout_ticks,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn postgres_prepare(
        &mut self,
        owner: VmProcessId,
        connection: VmPostgresConnection,
        sql: impl Into<String>,
        parameter_count: usize,
        now_tick: u64,
        timeout_ticks: u64,
    ) -> Result<RequestId, String> {
        self.postgres.prepare(
            &mut self.timers,
            &mut self.processes,
            &mut self.scheduler,
            owner,
            connection,
            sql,
            parameter_count,
            now_tick,
            timeout_ticks,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn postgres_decode(
        &mut self,
        owner: VmProcessId,
        row: VmPostgresRow,
        column: impl Into<String>,
        expected: VmPostgresDecodeType,
        now_tick: u64,
        timeout_ticks: u64,
    ) -> Result<RequestId, String> {
        self.postgres.decode(
            &mut self.timers,
            &mut self.processes,
            &mut self.scheduler,
            owner,
            row,
            column,
            expected,
            now_tick,
            timeout_ticks,
        )
    }

    pub(crate) fn take_postgres_dispatch(&mut self) -> Option<VmPostgresDriverRequest> {
        self.postgres.take_dispatch()
    }

    pub(crate) fn drive_postgres_once(&mut self) -> Result<Option<RequestId>, String> {
        while let Some(request) = self.postgres.take_dispatch() {
            self.postgres_driver.submit(request);
        }
        let Some((request_id, completion)) = self.postgres_driver.drive_once() else {
            return Ok(None);
        };
        self.complete_postgres(request_id, completion)?;
        Ok(Some(request_id))
    }

    pub(crate) fn drive_postgres_socket_ready(
        &mut self,
        ready: &std::collections::BTreeSet<u64>,
    ) -> Result<Option<RequestId>, String> {
        while let Some(request) = self.postgres.take_dispatch() {
            self.postgres_driver.submit(request);
        }
        let Some((request_id, completion)) = self.postgres_driver.drive_socket_ready(ready) else {
            return Ok(None);
        };
        self.complete_postgres(request_id, completion)?;
        Ok(Some(request_id))
    }

    pub(crate) fn complete_postgres(
        &mut self,
        request_id: RequestId,
        completion: VmPostgresDriverCompletion,
    ) -> Result<(), String> {
        self.postgres.complete(
            &mut self.timers,
            &mut self.processes,
            &mut self.scheduler,
            request_id,
            completion,
        )?;
        while let Some(control) = self.postgres.take_completion_control() {
            self.postgres_controls.push_back(control);
        }
        Ok(())
    }

    pub(crate) fn cancel_postgres(&mut self, request_id: RequestId) -> Result<(), String> {
        let control = self.postgres.cancel(
            &mut self.timers,
            &mut self.processes,
            &mut self.scheduler,
            request_id,
        )?;
        self.postgres_controls.push_back(control);
        Ok(())
    }

    pub(crate) fn take_postgres_reply(
        &mut self,
        owner: VmProcessId,
        request_id: RequestId,
    ) -> Result<VmPostgresReply, String> {
        self.postgres.take_reply(owner, request_id)
    }

    pub(crate) fn take_postgres_control(&mut self) -> Option<VmPostgresDriverControl> {
        self.postgres_controls.pop_front()
    }

    pub(crate) fn postgres_driver_wait(
        &self,
    ) -> Option<crate::runtime::vm::postgres::VmPostgresDriverWait> {
        self.postgres_driver.wait()
    }

    pub(crate) fn postgres_driver_waits(
        &self,
    ) -> Vec<crate::runtime::vm::postgres::VmPostgresDriverWait> {
        self.postgres_driver.waits()
    }

    pub(crate) fn wait_postgres_ready(
        &self,
        poller: &crate::terlan_native::postgres::libpq::DriverReadinessPoller,
        timeout: Option<std::time::Duration>,
    ) -> Result<std::collections::BTreeSet<u64>, String> {
        self.postgres_driver
            .wait_ready(poller, timeout)
            .map_err(|error| format!("error[{}]: {}", error.code(), error.message()))
    }

    pub(crate) fn drive_postgres_controls(&mut self) -> Result<usize, String> {
        let mut processed = 0;
        let mut first_error = None;
        while let Some(control) = self.postgres_controls.pop_front() {
            match self.postgres_driver.apply_control(control) {
                Ok(()) => processed += 1,
                Err(error) if first_error.is_none() => {
                    first_error = Some(format!("error[{}]: {}", error.code, error.message));
                }
                Err(_) => {}
            }
        }
        if let Some(error) = first_error {
            Err(error)
        } else {
            Ok(processed)
        }
    }

    pub(crate) fn write_postgres_report(&self, path: &Path) -> Result<(), String> {
        self.postgres.write_report(path)
    }

    pub(super) fn consume_postgres_timer_event(
        &mut self,
        event: &VmTimerEvent,
    ) -> Result<Option<VmPostgresDriverControl>, String> {
        let control =
            self.postgres
                .handle_timer_event(&mut self.processes, &mut self.scheduler, event)?;
        if let Some(control) = control {
            self.postgres_controls.push_back(control);
        }
        Ok(control)
    }

    pub(super) fn cleanup_postgres_owner(&mut self, owner: VmProcessId) {
        self.postgres_controls
            .extend(self.postgres.cleanup_owner(owner));
    }
}

#[cfg(test)]
#[path = "actor_postgres_control_test.rs"]
mod actor_postgres_control_test;
