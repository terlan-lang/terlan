//! VM process/table mechanics for package-owned session lifecycle decisions.

use super::*;
use terlan_http_native::session_registry::{SessionError, SessionResources};

pub(super) struct Resources<'a> {
    pub(super) actors: &'a mut VmActorRuntime,
    pub(super) tables: &'a mut VmTableStore,
}

impl SessionResources<VmHttpSessionResource> for Resources<'_> {
    fn create(&mut self, identity: &str) -> Result<VmHttpSessionResource, SessionError> {
        let actor = self
            .actors
            .spawn_root(VmProcessSource::new("std.http.Session", "actor", 0));
        let table = match self.tables.create(
            self.actors.processes(),
            actor,
            format!("http_session:{identity}"),
            VmTableAccess::OwnerOnly,
        ) {
            Ok(event) => created_session_table_id(event),
            Err(error) => {
                self.actors
                    .exit_actor(actor, VmExitReason::Normal)
                    .map_err(SessionError::Resource)?;
                return Err(SessionError::Resource(error));
            }
        };
        Ok(VmHttpSessionResource {
            actor,
            table,
            state_version: 0,
            command_results: BTreeMap::new(),
            live_template_subscribers: BTreeMap::new(),
        })
    }

    fn failure(&self, identity: &str, value: &VmHttpSessionResource) -> Option<SessionError> {
        exit_reason(self.actors, value.actor).map(|reason| {
            SessionError::Resource(crashed_session_actor_diagnostic(
                identity,
                value.actor,
                &reason,
            ))
        })
    }

    fn release(&mut self, value: &VmHttpSessionResource) -> Result<(), SessionError> {
        let result = if exit_reason(self.actors, value.actor).is_none() {
            self.actors
                .exit_actor(value.actor, VmExitReason::Normal)
                .map(|_| ())
        } else {
            Ok(())
        };
        self.tables.cleanup_owner(value.actor);
        result.map_err(SessionError::Resource)
    }
}

pub(super) fn exit_reason(actors: &VmActorRuntime, actor: VmProcessId) -> Option<VmExitReason> {
    match actors.processes().get(actor) {
        Some(process) => match &process.state {
            VmProcessState::Exited(reason) => Some(reason.clone()),
            VmProcessState::Runnable
            | VmProcessState::Blocked
            | VmProcessState::Hibernated
            | VmProcessState::Suspended(_) => None,
        },
        None => Some(VmExitReason::Error("missing actor process".to_string())),
    }
}
