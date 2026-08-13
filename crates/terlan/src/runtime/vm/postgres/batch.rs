use super::completion::validate_sql;
use super::{
    VmPostgresDriverOperation, VmPostgresQueryTarget, VmPostgresRequestContext, VmPostgresRuntime,
};
use crate::terlan_native_boundary::request::RequestId;

impl VmPostgresRuntime {
    /// Submits a trusted multi-statement SQL batch to the maintained driver.
    pub(crate) fn batch_execute(
        &mut self,
        context: VmPostgresRequestContext<'_>,
        target: VmPostgresQueryTarget,
        sql: impl Into<String>,
    ) -> Result<RequestId, String> {
        let owner = context.owner;
        let driver_target = self.driver_target(target, owner)?;
        let sql = validate_sql(sql.into())?;
        self.submit(
            context,
            VmPostgresDriverOperation::BatchExecute {
                target,
                driver_target,
                sql,
            },
        )
    }
}
