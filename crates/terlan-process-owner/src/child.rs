//! Request-scoped child and Linux/macOS process-group cleanup.

use std::io;
use std::process::{Child, Command, ExitStatus};

use crate::inventory::Inventory;

// Keep observation I/O errors distinct from the operating system's spawn result.
enum SpawnFailure {
    Observation(io::Error),
    Launch(io::Error),
}

impl From<SpawnFailure> for io::Error {
    fn from(error: SpawnFailure) -> Self {
        match error {
            SpawnFailure::Observation(error) | SpawnFailure::Launch(error) => error,
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
use rustix::process::{kill_process_group, waitid, Pid, Signal, WaitId, WaitIdOptions};

/// Owns a child until termination and reaping, including cleanup on unwinding.
pub struct OwnedChild {
    child: Child,
    finished: bool,
    inventory: Inventory,
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    owns_group: bool,
}

impl OwnedChild {
    /// Returns the direct child's operating-system identifier for attribution.
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// Spawns a command in its own group on Linux/macOS.
    pub fn spawn(mut command: Command) -> io::Result<Self> {
        Self::spawn_command(&mut command)
    }

    /// Spawns through a borrowed builder while retaining cleanup ownership.
    pub fn spawn_command(command: &mut Command) -> io::Result<Self> {
        Self::spawn_isolated(command).map_err(Into::into)
    }

    /// Returns `None` only for an observed OS spawn `NotFound` result.
    ///
    /// Observation/admission failures remain errors, even when their I/O kind is
    /// `NotFound`. Successful children retain the ordinary cleanup ownership.
    pub fn spawn_optional_command(command: &mut Command) -> io::Result<Option<Self>> {
        match Self::spawn_isolated(command) {
            Ok(child) => Ok(Some(child)),
            Err(SpawnFailure::Launch(error)) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn spawn_isolated(command: &mut Command) -> Result<Self, SpawnFailure> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        Self::spawn_recorded(command, true)
    }

    /// Keeps a nested child inside an explicitly identified enclosing owner's group.
    ///
    /// The caller must already belong to that group. The enclosing owner must
    /// retain its leader and own whole-group cleanup; this owner kills/reaps only
    /// its direct child. In particular, inner cleanup must never kill siblings
    /// or move a child outside the enclosing owner's cancellation boundary.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub fn spawn_in_enclosing_group(command: &mut Command, owner: u32) -> io::Result<Self> {
        use std::os::unix::process::CommandExt;
        let owner = i32::try_from(owner)
            .ok()
            .and_then(Pid::from_raw)
            .filter(|owner| *owner == rustix::process::getpgrp())
            .ok_or_else(|| {
                io::Error::other("caller does not belong to the enclosing owner group")
            })?;
        // Override a reused builder's previous process_group(0), too.
        command.process_group(owner.as_raw_pid());
        Self::spawn_recorded(command, false).map_err(Into::into)
    }

    fn spawn_recorded(command: &mut Command, owns_group: bool) -> Result<Self, SpawnFailure> {
        let mut inventory = Inventory::begin(command).map_err(SpawnFailure::Observation)?;
        let child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                inventory
                    .spawn_failed(&error)
                    .map_err(SpawnFailure::Observation)?;
                return Err(SpawnFailure::Launch(error));
            }
        };
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let _ = owns_group;
        let mut owner = Self {
            child,
            finished: false,
            inventory,
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            owns_group,
        };
        owner
            .inventory
            .spawned(owner.child.id())
            .map_err(SpawnFailure::Observation)?;
        Ok(owner)
    }

    /// Transfers the child's configured stdin pipe to its I/O owner.
    pub fn take_stdin(&mut self) -> Option<std::process::ChildStdin> {
        self.child.stdin.take()
    }

    /// Transfers the child's configured stdout pipe to its I/O owner.
    pub fn take_stdout(&mut self) -> Option<std::process::ChildStdout> {
        self.child.stdout.take()
    }

    /// Transfers the child's configured stderr pipe to its I/O owner.
    pub fn take_stderr(&mut self) -> Option<std::process::ChildStderr> {
        self.child.stderr.take()
    }

    /// Observes exit, performs the owned scope's cleanup, and reaps the retained child.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if self.finished {
            return self.child.try_wait();
        }
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            // Keep the leader waitable until group cleanup so PID/PGID reuse
            // cannot redirect cleanup at another job.
            let pid = Pid::from_raw(self.child.id() as i32)
                .ok_or_else(|| io::Error::other("invalid child process identity"))?;
            match waitid(
                WaitId::Pid(pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            ) {
                Ok(None) => Ok(None),
                Ok(Some(_)) => self.finish().map(Some),
                Err(rustix::io::Errno::INTR) => Ok(None),
                Err(error) => Err(error.into()),
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let status = self.child.try_wait()?;
            self.finished = status.is_some();
            if let Some(status) = status {
                self.inventory.reaped(status)?;
            }
            Ok(status)
        }
    }

    /// Terminates remaining owned processes before reaping the direct child.
    pub fn finish(&mut self) -> io::Result<ExitStatus> {
        if !self.finished {
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            let group_result = if self.owns_group {
                let pid = Pid::from_raw(self.child.id() as i32)
                    .ok_or_else(|| io::Error::other("invalid child process identity"))?;
                match kill_process_group(pid, Signal::KILL) {
                    Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
                    Err(error) => self.check_group_error(pid, error),
                }
            } else {
                Ok(())
            };
            // A child may have moved to a different process group. Its PID is
            // still owned and unreaped, so terminate it directly as well. Never
            // wait indefinitely merely because the original group disappeared.
            // In particular, do not follow it by killing its new (foreign) group.
            self.child.kill()?;
            let status = self.child.wait()?;
            self.finished = true;
            let recorded = self.inventory.reaped(status);
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            group_result?;
            recorded?;
            return Ok(status);
        }
        self.child.wait()
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn check_group_error(&self, pid: Pid, error: rustix::io::Errno) -> io::Result<()> {
        #[cfg(target_os = "macos")]
        if error == rustix::io::Errno::PERM {
            // Darwin's killpg1 excludes zombies, then returns EPERM when it
            // found no signalable members. Do not suppress permission errors
            // for live children or groups containing any other process.
            // NOWAIT retains our PID/PGID throughout this kernel inventory.
            let exited = waitid(
                WaitId::Pid(pid),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            )?
            .is_some();
            if exited {
                use libproc::processes::{pids_by_type, ProcFilter};
                let members = pids_by_type(ProcFilter::ByProgramGroup {
                    pgrpid: self.child.id(),
                })?;
                if members.as_slice() == [self.child.id()] {
                    return Ok(());
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = pid;
        Err(io::Error::from(error))
    }
}

#[cfg(test)]
#[path = "child_test.rs"]
mod tests;

#[cfg(test)]
#[path = "enclosing_group_test.rs"]
mod enclosing_group_test;

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.finish();
        }
    }
}
