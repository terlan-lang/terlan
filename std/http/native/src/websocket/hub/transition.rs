//! Per-room source execution with revision-checked publication and no replay.

use super::*;
use crate::source_descriptor::{PairContext, PairedTransition};

impl WebSocketHub {
    pub(super) async fn transition_pair<F, R>(
        &self,
        id: u64,
        transition: F,
    ) -> Result<(), crate::ServiceError>
    where
        F: FnOnce(Option<PairContext>) -> R,
        R: Future<Output = Result<PairedTransition, crate::ServiceError>>,
    {
        let (pair_id, execution) = {
            let state = self.lock_state()?;
            let (session, pair) = current(&state, id)?;
            (
                session.pair,
                Arc::clone(pair.map_or(&session.execution, |pair| &pair.execution)),
            )
        };
        let _permit = execution.lock().await;
        let (revision, context) = {
            let state = self.lock_state()?;
            let (session, pair) = current(&state, id)?;
            if session.pair != pair_id {
                return Err(changed());
            }
            let revision = pair.map(|pair| pair.revision);
            if revision == Some(u64::MAX) {
                return Err("error[serve.websocket.pairing]: room revision exhausted".into());
            }
            let context = pair.map(|pair| {
                (
                    pair.state.clone(),
                    session.role,
                    pair.first_request.clone(),
                    pair.second_request.clone(),
                )
            });
            (revision, context)
        };
        // Source effects execute exactly once. Membership changes invalidate
        // their result rather than replaying effects against a new snapshot.
        let (next, first, second) = transition(context).await?;
        let mut state = self.lock_state()?;
        let (session, pair) = current(&state, id)?;
        if session.pair != pair_id || pair.map(|pair| pair.revision) != revision {
            return Err(changed());
        }
        let recipients = if let Some(pair_id) = pair_id {
            let pair = state.pairs.get_mut(&pair_id).expect("validated pair");
            pair.state = next;
            pair.revision += 1;
            [pair.first, pair.second]
        } else {
            [Some(id), None]
        };
        // Nonblocking sends remain ordered with state commits and departures.
        // Delivery to both peers is not an atomic multi-queue operation.
        for (recipient, payload) in recipients.into_iter().zip([first, second]) {
            if let (Some(session), Some(payload)) =
                (recipient.and_then(|id| state.sessions.get(&id)), payload)
            {
                send_hub_payload(&session.outbound, payload)?;
            }
        }
        Ok(())
    }

    fn lock_state(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, WebSocketHubState>, crate::ServiceError> {
        self.state
            .lock()
            .map_err(|_| "error[serve.websocket.hub]: pairing state lock poisoned".into())
    }
}

fn current(
    state: &WebSocketHubState,
    id: u64,
) -> Result<(&WebSocketHubSession, Option<&WebSocketHubPair>), crate::ServiceError> {
    let session = state
        .sessions
        .get(&id)
        .ok_or_else(|| "error[serve.websocket.hub]: session is no longer registered".to_string())?;
    let pair = session
        .pair
        .map(|id| {
            state.pairs.get(&id).ok_or_else(|| {
                "error[serve.websocket.pairing]: paired session state disappeared".to_string()
            })
        })
        .transpose()?;
    Ok((session, pair))
}

fn changed() -> crate::ServiceError {
    "error[serve.websocket.pairing]: room changed during source transition".into()
}
