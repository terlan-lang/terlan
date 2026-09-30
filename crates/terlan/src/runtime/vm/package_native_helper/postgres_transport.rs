//! Database policy over the shared actor-scoped resource transport.
use super::{postgres, resource_transport};
use crate::runtime::vm::pure_native::{PureNativeCapabilityWait, PureNativeSuspension};

/// VM execution state retained while a database worker runs.
pub(super) struct Continuation {
    pub(super) suspension: PureNativeSuspension,
    pub(super) wait: PureNativeCapabilityWait,
}
pub(super) type Pending<T = Continuation> = resource_transport::Pending<T, postgres::Projection>;
pub(super) type Workers<T = Continuation> = resource_transport::Workers<T, postgres::Projection>;

impl<T> Default for Workers<T> {
    fn default() -> Self {
        Self::new(resource_transport::Policy {
            capability: "postgres",
            worker_class: "blocking",
            identity: "postgres",
            error_code: "postgres.indeterminate",
            capacity_error_code: "postgres.resource_limit",
            loss_context: "a database mutation may have occurred; do not retry automatically",
        })
    }
}
