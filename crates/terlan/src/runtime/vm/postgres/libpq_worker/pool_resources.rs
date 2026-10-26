//! Resource operations for VmPostgresLibpqWorker.

use super::*;

impl VmPostgresLibpqWorker {
    pub(super) fn request_conflicts(&self, request: &VmPostgresDriverRequest) -> bool {
        self.active
            .values()
            .any(|pending| pending.conflicts_with(&request.operation))
    }

    pub(super) fn pool_mut(
        &mut self,
        handle: VmPostgresDriverPool,
    ) -> Result<&mut PoolEntry, postgres::PostgresError> {
        self.pools.get_mut(&handle).ok_or_else(|| stale("pool"))
    }

    pub(super) fn allocate_resource_id(&mut self) -> Result<u64, postgres::PostgresError> {
        let id = self.next_resource_id.max(1);
        self.next_resource_id = id.checked_add(1).ok_or_else(|| {
            postgres::PostgresError::new(
                "postgres.driver.resource_overflow",
                "Postgres driver resource id overflow.",
            )
        })?;
        Ok(id)
    }
}
