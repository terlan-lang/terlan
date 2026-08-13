use std::collections::{HashMap, VecDeque};

use super::process::VmProcessId;

#[cfg(test)]
#[path = "debugger_transport_test.rs"]
#[cfg(test)]
mod debugger_transport_test;

/// VM-owned debugger transport session handle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct VmDebuggerSession {
    id: u64,
}

/// Debugger command delivered through the VM reactor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmDebuggerCommand {
    Continue,
    Step,
    SetBreakpoint { source_map_id: String, line: u32 },
}

/// Debugger event emitted through the VM reactor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmDebuggerEvent {
    Stopped {
        process: VmProcessId,
        reason: String,
    },
    Output(String),
    Diagnostic(String),
}

/// Runtime-visible debugger transport wake intent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmDebuggerWake {
    Command {
        process: VmProcessId,
        session: VmDebuggerSession,
    },
    Event {
        process: VmProcessId,
        session: VmDebuggerSession,
    },
}

/// Runtime-visible debugger session state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmDebuggerSessionInfo {
    pub(crate) owner: String,
    pub(crate) queued_commands: usize,
    pub(crate) queued_events: usize,
    pub(crate) command_limit: usize,
    pub(crate) event_limit: usize,
    pub(crate) waiting_command_receivers: usize,
    pub(crate) waiting_event_receivers: usize,
    pub(crate) closed: bool,
}

/// VM-owned debugger transport registry.
#[derive(Debug, Default)]
pub(crate) struct VmDebuggerTransportRuntime {
    next_session: u64,
    sessions: HashMap<u64, DebuggerSessionState>,
}

#[derive(Debug)]
struct DebuggerSessionState {
    owner: String,
    commands: VecDeque<VmDebuggerCommand>,
    events: VecDeque<VmDebuggerEvent>,
    command_limit: usize,
    event_limit: usize,
    command_waiters: VecDeque<VmProcessId>,
    event_waiters: VecDeque<VmProcessId>,
    closed: bool,
}

impl VmDebuggerTransportRuntime {
    /// Creates an empty debugger transport runtime.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Opens a bounded VM-owned debugger session.
    pub(crate) fn open_session(
        &mut self,
        owner: impl Into<String>,
        command_limit: usize,
        event_limit: usize,
    ) -> Result<VmDebuggerSession, String> {
        if command_limit == 0 {
            return Err("VM debugger command queue limit must be greater than 0".to_string());
        }
        if event_limit == 0 {
            return Err("VM debugger event queue limit must be greater than 0".to_string());
        }
        self.next_session = self.next_session.saturating_add(1);
        let session = VmDebuggerSession {
            id: self.next_session,
        };
        self.sessions.insert(
            session.id,
            DebuggerSessionState {
                owner: owner.into(),
                commands: VecDeque::new(),
                events: VecDeque::new(),
                command_limit,
                event_limit,
                command_waiters: VecDeque::new(),
                event_waiters: VecDeque::new(),
                closed: false,
            },
        );
        Ok(session)
    }

    /// Enqueues one debugger command and wakes blocked VM command receivers.
    pub(crate) fn enqueue_command(
        &mut self,
        session: VmDebuggerSession,
        command: VmDebuggerCommand,
    ) -> Result<Vec<VmDebuggerWake>, String> {
        Self::validate_command(&command)?;
        let state = self.session_mut(session)?;
        if state.commands.len() >= state.command_limit {
            return Err("VM debugger command queue is full".to_string());
        }
        state.commands.push_back(command);
        Ok(state
            .command_waiters
            .drain(..)
            .map(|process| VmDebuggerWake::Command { process, session })
            .collect())
    }

    /// Receives the next debugger command for a VM process.
    pub(crate) fn receive_command(
        &mut self,
        session: VmDebuggerSession,
    ) -> Result<Option<VmDebuggerCommand>, String> {
        Ok(self.session_mut(session)?.commands.pop_front())
    }

    /// Parks a VM process waiting for debugger commands.
    pub(crate) fn park_command_receive(
        &mut self,
        session: VmDebuggerSession,
        process: VmProcessId,
    ) -> Result<bool, String> {
        let state = self.session_mut(session)?;
        if !state.commands.is_empty() {
            return Ok(false);
        }
        if !state.command_waiters.contains(&process) {
            state.command_waiters.push_back(process);
        }
        Ok(true)
    }

    /// Enqueues one debugger event and wakes blocked debugger event receivers.
    pub(crate) fn enqueue_event(
        &mut self,
        session: VmDebuggerSession,
        event: VmDebuggerEvent,
    ) -> Result<Vec<VmDebuggerWake>, String> {
        Self::validate_event(&event)?;
        let state = self.session_mut(session)?;
        if state.events.len() >= state.event_limit {
            return Err("VM debugger event queue is full".to_string());
        }
        state.events.push_back(event);
        Ok(state
            .event_waiters
            .drain(..)
            .map(|process| VmDebuggerWake::Event { process, session })
            .collect())
    }

    /// Receives the next debugger event.
    pub(crate) fn receive_event(
        &mut self,
        session: VmDebuggerSession,
    ) -> Result<Option<VmDebuggerEvent>, String> {
        Ok(self.session_mut(session)?.events.pop_front())
    }

    /// Parks a debugger process waiting for events from the VM.
    pub(crate) fn park_event_receive(
        &mut self,
        session: VmDebuggerSession,
        process: VmProcessId,
    ) -> Result<bool, String> {
        let state = self.session_mut(session)?;
        if !state.events.is_empty() {
            return Ok(false);
        }
        if !state.event_waiters.contains(&process) {
            state.event_waiters.push_back(process);
        }
        Ok(true)
    }

    /// Closes one debugger session and clears queued transport state.
    pub(crate) fn close_session(&mut self, session: VmDebuggerSession) -> Result<(), String> {
        let state = self.session_mut(session)?;
        state.commands.clear();
        state.events.clear();
        state.command_waiters.clear();
        state.event_waiters.clear();
        state.closed = true;
        Ok(())
    }

    /// Closes all debugger sessions owned by one actor/runtime owner.
    pub(crate) fn close_owner_sessions(&mut self, owner: &str) -> Vec<VmDebuggerSession> {
        let sessions = self
            .sessions
            .iter()
            .filter_map(|(id, state)| {
                (!state.closed && state.owner == owner).then_some(VmDebuggerSession { id: *id })
            })
            .collect::<Vec<_>>();
        for session in &sessions {
            let _ = self.close_session(*session);
        }
        sessions
    }

    /// Returns an inspectable debugger session snapshot.
    pub(crate) fn inspect_session(
        &self,
        session: VmDebuggerSession,
    ) -> Result<VmDebuggerSessionInfo, String> {
        let state = self.session(session)?;
        Ok(VmDebuggerSessionInfo {
            owner: state.owner.clone(),
            queued_commands: state.commands.len(),
            queued_events: state.events.len(),
            command_limit: state.command_limit,
            event_limit: state.event_limit,
            waiting_command_receivers: state.command_waiters.len(),
            waiting_event_receivers: state.event_waiters.len(),
            closed: state.closed,
        })
    }

    fn validate_command(command: &VmDebuggerCommand) -> Result<(), String> {
        if let VmDebuggerCommand::SetBreakpoint {
            source_map_id,
            line,
        } = command
        {
            if source_map_id.trim().is_empty() {
                return Err("VM debugger breakpoint source_map_id cannot be empty".to_string());
            }
            if *line == 0 {
                return Err("VM debugger breakpoint line must be one-based".to_string());
            }
        }
        Ok(())
    }

    fn validate_event(event: &VmDebuggerEvent) -> Result<(), String> {
        match event {
            VmDebuggerEvent::Stopped { reason, .. } if reason.trim().is_empty() => {
                Err("VM debugger stopped reason cannot be empty".to_string())
            }
            VmDebuggerEvent::Output(output) if output.is_empty() => {
                Err("VM debugger output event cannot be empty".to_string())
            }
            VmDebuggerEvent::Diagnostic(diagnostic) if diagnostic.trim().is_empty() => {
                Err("VM debugger diagnostic event cannot be empty".to_string())
            }
            _ => Ok(()),
        }
    }

    fn session(&self, session: VmDebuggerSession) -> Result<&DebuggerSessionState, String> {
        let state = self
            .sessions
            .get(&session.id)
            .ok_or_else(|| "VM debugger session was not found".to_string())?;
        if state.closed {
            return Err("VM debugger session is closed".to_string());
        }
        Ok(state)
    }

    fn session_mut(
        &mut self,
        session: VmDebuggerSession,
    ) -> Result<&mut DebuggerSessionState, String> {
        let state = self
            .sessions
            .get_mut(&session.id)
            .ok_or_else(|| "VM debugger session was not found".to_string())?;
        if state.closed {
            return Err("VM debugger session is closed".to_string());
        }
        Ok(state)
    }
}
