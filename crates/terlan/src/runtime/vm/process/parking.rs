use super::{VmProcess, VmProcessResumeState, VmProcessState};

impl VmProcess {
    /// Wakes a blocked or hibernated process.
    pub(crate) fn wake(&mut self) {
        match self.state {
            VmProcessState::Blocked | VmProcessState::Hibernated => {
                self.state = VmProcessState::Runnable;
            }
            VmProcessState::Suspended(VmProcessResumeState::Blocked)
            | VmProcessState::Suspended(VmProcessResumeState::Hibernated) => {
                self.state = VmProcessState::Suspended(VmProcessResumeState::Runnable);
            }
            VmProcessState::Runnable
            | VmProcessState::Suspended(VmProcessResumeState::Runnable)
            | VmProcessState::Exited(_) => {}
        }
    }

    /// Parks a live process until its next VM-owned wake event.
    pub(crate) fn hibernate(&mut self) -> Result<(), String> {
        self.state = match self.state {
            VmProcessState::Runnable | VmProcessState::Blocked | VmProcessState::Hibernated => {
                VmProcessState::Hibernated
            }
            VmProcessState::Suspended(_) => {
                return Err("cannot hibernate explicitly suspended process".to_string());
            }
            VmProcessState::Exited(_) => return Err("cannot hibernate exited process".to_string()),
        };
        Ok(())
    }

    /// Suspends a live process while retaining the state restored by resume.
    pub(crate) fn suspend(&mut self) -> Result<(), String> {
        self.state = match self.state {
            VmProcessState::Runnable => VmProcessState::Suspended(VmProcessResumeState::Runnable),
            VmProcessState::Blocked => VmProcessState::Suspended(VmProcessResumeState::Blocked),
            VmProcessState::Hibernated => {
                VmProcessState::Suspended(VmProcessResumeState::Hibernated)
            }
            VmProcessState::Suspended(resume_state) => VmProcessState::Suspended(resume_state),
            VmProcessState::Exited(_) => return Err("cannot suspend exited process".to_string()),
        };
        Ok(())
    }

    /// Resumes a suspended process to its retained execution state.
    pub(crate) fn resume(&mut self) -> Result<VmProcessResumeState, String> {
        let VmProcessState::Suspended(resume_state) = self.state else {
            return Err("process is not suspended".to_string());
        };
        self.state = match resume_state {
            VmProcessResumeState::Runnable => VmProcessState::Runnable,
            VmProcessResumeState::Blocked => VmProcessState::Blocked,
            VmProcessResumeState::Hibernated => VmProcessState::Hibernated,
        };
        Ok(resume_state)
    }
}
