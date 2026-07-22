use super::{validate_sql, VmPostgresDriverOperation, VmPostgresQueryTarget, VmPostgresRuntime};
use crate::runtime::vm::{
    process::{VmProcessId, VmProcessTable},
    scheduler::VmScheduler,
    timer::VmTimerTable,
};
use crate::terlan_native_boundary::request::RequestId;

impl VmPostgresRuntime {
    /// Submits a trusted multi-statement SQL batch to the maintained driver.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn batch_execute(
        &mut self,
        timers: &mut VmTimerTable,
        processes: &mut VmProcessTable,
        scheduler: &mut VmScheduler,
        owner: VmProcessId,
        target: VmPostgresQueryTarget,
        sql: impl Into<String>,
        now_tick: u64,
        timeout_ticks: u64,
    ) -> Result<RequestId, String> {
        let driver_target = self.driver_target(target, owner)?;
        let sql = validate_sql(sql.into())?;
        self.submit(
            timers,
            processes,
            scheduler,
            owner,
            VmPostgresDriverOperation::BatchExecute {
                target,
                driver_target,
                sql,
            },
            now_tick,
            timeout_ticks,
        )
    }
}
