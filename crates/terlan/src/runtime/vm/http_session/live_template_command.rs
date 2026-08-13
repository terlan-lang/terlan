use super::*;

/// Validated live-template command envelope delivered to a session actor.
#[derive(Clone, Debug, PartialEq)]
#[cfg(test)]
pub(crate) struct VmHttpSessionCommandPayload {
    pub(crate) command_id: String,
    pub(crate) name: String,
    pub(crate) body: ReplValue,
}

/// Typed live-template command consumed from a session actor mailbox.
#[derive(Clone, Debug, PartialEq)]
#[cfg(test)]
pub(crate) struct VmHttpSessionLiveTemplateActorCommand {
    pub(crate) command_id: String,
    pub(crate) name: String,
    pub(crate) body: ReplValue,
}

#[cfg(test)]
impl VmHttpSessionCommandPayload {
    pub(crate) fn new(command_id: &str, name: &str, body: ReplValue) -> Result<Self, String> {
        Ok(Self {
            command_id: normalize_command_id(command_id)?.to_string(),
            name: normalize_command_name(name)?.to_string(),
            body,
        })
    }
}

impl VmHttpSessionRuntime {
    /// Validates and applies one live-template command before actor dispatch.
    #[cfg(test)]
    pub(crate) fn apply_live_template_command(
        &mut self,
        session: &VmHttpSession,
        command_id: &str,
        name: &str,
        body: ReplValue,
        command: impl FnOnce(
            &mut Self,
            &VmHttpSession,
            VmHttpSessionCommandPayload,
        ) -> Result<ReplValue, String>,
    ) -> Result<VmHttpSessionCommandOutcome, String> {
        let payload = VmHttpSessionCommandPayload::new(command_id, name, body)?;
        let command_id = payload.command_id.clone();
        self.apply_idempotent_command(session, &command_id, |runtime, session| {
            command(runtime, session, payload)
        })
    }

    /// Dispatches one browser command postback into the session actor mailbox.
    #[cfg(test)]
    pub(crate) fn dispatch_live_template_command_to_actor_mailbox(
        &mut self,
        session: &VmHttpSession,
        command_id: &str,
        name: &str,
        body: ReplValue,
    ) -> Result<VmHttpSessionCommandOutcome, String> {
        let payload = VmHttpSessionCommandPayload::new(command_id, name, body)?;
        let command_id = payload.command_id.clone();
        self.apply_idempotent_command(session, &command_id, |runtime, session| {
            let message = live_template_command_actor_message(payload);
            let message_id = runtime.enqueue_actor_message(session, message)?;
            let message_id = http_message_id_to_int(message_id)?;
            Ok(ReplValue::Tuple(vec![
                ReplValue::Atom("live_template_command_dispatched".to_string()),
                ReplValue::Int(message_id),
            ]))
        })
    }

    /// Consumes one validated browser command from the session actor mailbox.
    #[cfg(test)]
    pub(crate) fn receive_live_template_actor_command(
        &mut self,
        session: &VmHttpSession,
    ) -> Result<Option<VmHttpSessionLiveTemplateActorCommand>, String> {
        let Some(message) = self.receive_next_actor_payload(session)? else {
            return Ok(None);
        };
        let ReplValue::Tuple(values) = message else {
            return Err(invalid_live_template_actor_command_diagnostic());
        };
        let [ReplValue::Atom(tag), ReplValue::String(command_id), ReplValue::String(name), body] =
            values.as_slice()
        else {
            return Err(invalid_live_template_actor_command_diagnostic());
        };
        if tag != "live_template_command" {
            return Err(invalid_live_template_actor_command_diagnostic());
        }
        Ok(Some(VmHttpSessionLiveTemplateActorCommand {
            command_id: command_id.clone(),
            name: name.clone(),
            body: body.clone(),
        }))
    }
}

#[cfg(test)]
fn live_template_command_actor_message(payload: VmHttpSessionCommandPayload) -> ReplValue {
    ReplValue::Tuple(vec![
        ReplValue::Atom("live_template_command".to_string()),
        ReplValue::String(payload.command_id),
        ReplValue::String(payload.name),
        payload.body,
    ])
}

#[cfg(test)]
fn invalid_live_template_actor_command_diagnostic() -> String {
    "invalid_live_template_actor_command: session actor mailbox message must be {live_template_command, command_id, name, body}"
        .to_string()
}
