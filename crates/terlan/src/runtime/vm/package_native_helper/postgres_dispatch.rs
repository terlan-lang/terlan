//! Source database requests shared by asynchronous server dispatchers.

use super::{postgres_transport, VmPackageNativeHelpers};
use crate::runtime::vm::capability_worker::VmCapabilityId;
use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::pure_native::{repl_value_to_boundary_term, PureNativeCapabilityWait};
use crate::runtime::vm::VmRuntimeResult;
use crate::terlan_native_boundary::term::NativeBoundaryReplyTerm;
use std::task::Waker;

/// Isolated database workers whose continuations remain with the server owner.
#[derive(Default)]
pub(crate) struct VmPostgresDispatcher {
    workers: postgres_transport::Workers<()>,
}

impl VmPostgresDispatcher {
    /// Translates checked source arguments and parks an actor-owned database call.
    pub(crate) fn submit(
        &mut self,
        helpers: &mut VmPackageNativeHelpers,
        owner: VmProcessId,
        wait: &PureNativeCapabilityWait,
        worker: std::path::PathBuf,
    ) -> VmRuntimeResult<bool> {
        let Some(request) = helpers.postgres.prepare(
            owner.as_u64(),
            wait.request(),
            &helpers.direct_std_resources,
        )?
        else {
            return Ok(false);
        };
        self.workers.select_worker(worker);
        let mut context = wait.worker_context()?;
        context.capability = VmCapabilityId::new("postgres")?;
        self.workers.submit(
            &request.operation,
            request.arguments,
            postgres_transport::Pending {
                owner,
                context,
                projection: request.projection,
                payload: (),
            },
        )?;
        Ok(true)
    }

    /// Arms transport notification before the owner polls its completion queue.
    pub(crate) fn register_waker(&self, owner: VmProcessId, waker: &Waker) {
        self.workers.register_owner_waker(owner, waker);
    }

    /// Projects one correlated reply into the declared source result representation.
    pub(crate) fn poll(
        &mut self,
        helpers: &mut VmPackageNativeHelpers,
    ) -> VmRuntimeResult<Option<(VmProcessId, NativeBoundaryReplyTerm)>> {
        let Some(completion) = self.workers.poll()? else {
            return Ok(None);
        };
        let pending = completion.pending;
        let reply = helpers
            .postgres
            .complete(
                pending.owner.as_u64(),
                pending.projection,
                completion.reply,
                &mut helpers.direct_std_resources,
            )
            .and_then(repl_value_to_boundary_term);
        let reply = match reply {
            Ok(value) => NativeBoundaryReplyTerm::Ok(value),
            Err(error) => NativeBoundaryReplyTerm::Error {
                code: "postgres.source_result".into(),
                message: error.to_string(),
                offset: 0,
            },
        };
        Ok(Some((pending.owner, reply)))
    }

    /// Revokes remote and local resources before the owning actor is released.
    pub(crate) fn close_owner(&mut self, helpers: &mut VmPackageNativeHelpers, owner: VmProcessId) {
        self.workers.close_owner(owner.as_u64());
        helpers.close_owner(owner.as_u64());
    }
}
