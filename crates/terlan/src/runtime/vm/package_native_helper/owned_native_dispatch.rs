//! Shared serving interface for actor-affine package and database workers.

use super::{
    postgres_dispatch::VmPostgresDispatcher, resource_projection, resource_transport,
    VmPackageNativeHelpers,
};
use crate::runtime::vm::{
    process::VmProcessId, pure_native::PureNativeCapabilityWait, VmRuntimeResult,
};
use crate::terlan_native_boundary::term::NativeBoundaryReplyTerm;
use std::{path::PathBuf, task::Waker};

pub(crate) struct VmOwnedNativeDispatcher {
    postgres: VmPostgresDispatcher,
    resources: resource_projection::Adapter,
    workers: resource_transport::Workers<(), resource_projection::Projection>,
}

impl Default for VmOwnedNativeDispatcher {
    fn default() -> Self {
        Self {
            postgres: VmPostgresDispatcher::default(),
            resources: resource_projection::Adapter::default(),
            workers: resource_transport::Workers::new(resource_transport::Policy {
                capability: "package-native",
                worker_class: "fast",
                identity: "package-resource",
                error_code: "resource_worker.lost",
                capacity_error_code: "resource_worker.resource_limit",
                loss_context: "actor resources are revoked; do not retry automatically",
            }),
        }
    }
}

impl VmOwnedNativeDispatcher {
    pub(crate) fn submit(
        &mut self,
        helpers: &mut VmPackageNativeHelpers,
        owner: VmProcessId,
        wait: &PureNativeCapabilityWait,
        worker: PathBuf,
    ) -> VmRuntimeResult<bool> {
        if crate::std_native_packages::resource_operation(&wait.request().operation).is_none() {
            return self.postgres.submit(helpers, owner, wait, worker);
        }
        let (arguments, projection) = self.resources.prepare(owner.as_u64(), wait.request())?;
        self.workers.select_worker(worker);
        self.workers.submit(
            &wait.request().operation,
            arguments,
            resource_transport::Pending {
                owner,
                payload: (),
                projection,
                context: wait.worker_context()?,
            },
        )?;
        Ok(true)
    }

    pub(crate) fn register_waker(&self, owner: VmProcessId, waker: &Waker) {
        self.postgres.register_waker(owner, waker);
        self.workers.register_owner_waker(owner, waker);
    }

    pub(crate) fn poll(
        &mut self,
        helpers: &mut VmPackageNativeHelpers,
    ) -> VmRuntimeResult<Option<(VmProcessId, NativeBoundaryReplyTerm)>> {
        if let Some(completion) = self.workers.poll()? {
            let owner = completion.pending.owner;
            let result = self.resources.complete(
                owner.as_u64(),
                completion.pending.projection,
                completion.reply,
            );
            let reply = match result {
                Ok(reply) => reply,
                Err(error) => {
                    self.workers.revoke_owner(owner.as_u64());
                    self.resources.close_owner(owner.as_u64());
                    NativeBoundaryReplyTerm::Error {
                        code: "native_package.reply".into(),
                        message: error.to_string(),
                        offset: 0,
                    }
                }
            };
            return Ok(Some((owner, reply)));
        }
        self.postgres.poll(helpers)
    }

    pub(crate) fn close_owner(&mut self, helpers: &mut VmPackageNativeHelpers, owner: VmProcessId) {
        self.workers.close_owner(owner.as_u64());
        self.resources.close_owner(owner.as_u64());
        self.postgres.close_owner(helpers, owner);
    }
}
