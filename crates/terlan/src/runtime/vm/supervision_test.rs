pub(super) use super::{
    VmChildRestartClass, VmChildSpec, VmRestartBackoffSchedule, VmRestartPolicy, VmShutdownTimeout,
    VmSupervisionRestart, VmSupervisionSystem, VmSupervisorRestartHistoryOutcome,
    VmSupervisorState,
};
pub(super) use crate::runtime::vm::process::{VmExitReason, VmProcessState, VmProcessTable};

#[cfg(test)]
#[path = "supervision_test/hierarchy_and_history.rs"]
mod hierarchy_and_history;
#[cfg(test)]
#[path = "supervision_test/restart_fixtures.rs"]
mod restart_fixtures;
use restart_fixtures::*;
