//! Bounded cross-connection delivery for source-declared paired WebSockets.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use url::form_urlencoded;

use crate::runtime::vm::websocket::VmWebSocketPairingPlan;
use crate::runtime::vm::ReplValue;

use super::super::handler::AotWebSocketCallbackSession;

#[derive(Default)]
pub(super) struct WebSocketHub {
    state: Mutex<WebSocketHubState>,
}

#[derive(Default)]
struct WebSocketHubState {
    next_id: u64,
    next_room: u64,
    waiting: HashMap<String, u64>,
    rooms: HashMap<(String, String), u64>,
    sessions: HashMap<u64, WebSocketHubSession>,
    pairs: HashMap<u64, WebSocketHubPair>,
}

struct WebSocketHubSession {
    outbound: SyncSender<String>,
    pair: Option<u64>,
    role: i64,
    peer_left: String,
}

struct WebSocketHubPair {
    first: Option<u64>,
    second: Option<u64>,
    first_request: String,
    second_request: String,
    state: String,
    route: String,
    room_id: Option<String>,
    retention: Option<Duration>,
    retained_room_capacity: usize,
    expires_at: Option<Instant>,
}

enum WebSocketHubAdmission {
    Waiting,
    Matched {
        pair_id: u64,
        room_id: String,
        first_request: String,
        second_request: String,
    },
    Restored {
        room_id: String,
        state: String,
        role: i64,
        first_request: String,
        second_request: String,
    },
}

pub(super) struct WebSocketHubLease {
    hub: Arc<WebSocketHub>,
    id: u64,
    admission: Option<WebSocketHubAdmission>,
    pub(super) outbound: Receiver<String>,
}

impl WebSocketHub {
    pub(super) fn join(
        self: &Arc<Self>,
        route: String,
        request_target: String,
        capacity: usize,
        pairing: &VmWebSocketPairingPlan,
    ) -> Result<WebSocketHubLease, String> {
        self.join_at(route, request_target, capacity, pairing, Instant::now())
    }

    pub(super) fn join_at(
        self: &Arc<Self>,
        route: String,
        request_target: String,
        capacity: usize,
        pairing: &VmWebSocketPairingPlan,
        now: Instant,
    ) -> Result<WebSocketHubLease, String> {
        let (outbound, receiver) = mpsc::sync_channel(capacity);
        let mut state = self
            .state
            .lock()
            .map_err(|_| "error[serve.websocket.hub]: pairing state lock poisoned".to_string())?;
        reap_expired_rooms(&mut state, now);
        state.next_id = state.next_id.saturating_add(1);
        let id = state.next_id;
        if let Some(restoration) = &pairing.restoration {
            if let Some((room_id, player_id)) = restore_identity(
                &request_target,
                &restoration.room_query,
                &restoration.player_query,
            )? {
                let role = if player_id == restoration.first_player {
                    1
                } else if player_id == restoration.second_player {
                    2
                } else {
                    return Err("error[serve.websocket.restore]: unknown player".to_string());
                };
                let pair_id = *state
                    .rooms
                    .get(&(route.clone(), room_id.clone()))
                    .ok_or_else(|| "error[serve.websocket.restore]: room not found".to_string())?;
                let (retained, first_request, second_request) = {
                    let pair = state.pairs.get_mut(&pair_id).ok_or_else(|| {
                        "error[serve.websocket.restore]: room state disappeared".to_string()
                    })?;
                    let seat = if role == 1 {
                        &mut pair.first
                    } else {
                        &mut pair.second
                    };
                    if seat.is_some() {
                        return Err(
                            "error[serve.websocket.restore]: player is already connected".into(),
                        );
                    }
                    *seat = Some(id);
                    pair.expires_at = None;
                    (
                        pair.state.clone(),
                        pair.first_request.clone(),
                        pair.second_request.clone(),
                    )
                };
                state.sessions.insert(
                    id,
                    WebSocketHubSession {
                        outbound,
                        pair: Some(pair_id),
                        role,
                        peer_left: pairing.peer_left.clone(),
                    },
                );
                return Ok(WebSocketHubLease {
                    hub: Arc::clone(self),
                    id,
                    admission: Some(WebSocketHubAdmission::Restored {
                        room_id,
                        state: retained,
                        role,
                        first_request,
                        second_request,
                    }),
                    outbound: receiver,
                });
            }
        }

        let waiting_id = state.waiting.remove(&route);
        state.sessions.insert(
            id,
            WebSocketHubSession {
                outbound: outbound.clone(),
                pair: None,
                role: if waiting_id.is_some() { 2 } else { 1 },
                peer_left: pairing.peer_left.clone(),
            },
        );
        let admission = if let Some(waiting_id) = waiting_id {
            let first_request = state
                .pairs
                .remove(&waiting_id)
                .ok_or_else(|| {
                    "error[serve.websocket.hub]: waiting metadata disappeared".to_string()
                })?
                .first_request;
            let (room_id, retention, retained_room_capacity) =
                if let Some(restoration) = &pairing.restoration {
                    state.next_room = state.next_room.saturating_add(1);
                    (
                        Some(format!("{}{}", restoration.room_prefix, state.next_room)),
                        Some(Duration::from_millis(restoration.retention_ms)),
                        restoration.retained_room_capacity,
                    )
                } else {
                    (None, None, 0)
                };
            state
                .sessions
                .get_mut(&waiting_id)
                .ok_or_else(|| {
                    "error[serve.websocket.hub]: waiting session disappeared".to_string()
                })?
                .pair = Some(waiting_id);
            state
                .sessions
                .get_mut(&id)
                .expect("new session exists")
                .pair = Some(waiting_id);
            state.pairs.insert(
                waiting_id,
                WebSocketHubPair {
                    first: Some(waiting_id),
                    second: Some(id),
                    first_request: first_request.clone(),
                    second_request: request_target.clone(),
                    state: String::new(),
                    route: route.clone(),
                    room_id: room_id.clone(),
                    retention,
                    retained_room_capacity,
                    expires_at: None,
                },
            );
            if let Some(room_id) = room_id {
                state.rooms.insert((route, room_id.clone()), waiting_id);
                WebSocketHubAdmission::Matched {
                    pair_id: waiting_id,
                    room_id,
                    first_request,
                    second_request: request_target,
                }
            } else {
                let first = state.sessions[&waiting_id].outbound.clone();
                send_hub_payload(&first, pairing.first_matched.clone())?;
                send_hub_payload(&outbound, pairing.second_matched.clone())?;
                WebSocketHubAdmission::Waiting
            }
        } else {
            state.waiting.insert(route.clone(), id);
            state.pairs.insert(
                id,
                WebSocketHubPair {
                    first: Some(id),
                    second: None,
                    first_request: request_target,
                    second_request: String::new(),
                    state: String::new(),
                    route: route.clone(),
                    room_id: None,
                    retention: None,
                    retained_room_capacity: 0,
                    expires_at: None,
                },
            );
            send_hub_payload(&outbound, pairing.waiting.clone())?;
            WebSocketHubAdmission::Waiting
        };
        Ok(WebSocketHubLease {
            hub: Arc::clone(self),
            id,
            admission: Some(admission),
            outbound: receiver,
        })
    }

    fn deliver(&self, id: u64, payload: String) -> Result<(), String> {
        let sender = self
            .state
            .lock()
            .map_err(|_| "error[serve.websocket.hub]: pairing state lock poisoned".to_string())?
            .sessions
            .get(&id)
            .map(|session| session.outbound.clone())
            .ok_or_else(|| {
                "error[serve.websocket.hub]: session is no longer registered".to_string()
            })?;
        send_hub_payload(&sender, payload)
    }

    fn complete_match(&self, pair_id: u64, first: String, second: String) -> Result<(), String> {
        let senders = {
            let state = self.state.lock().map_err(|_| {
                "error[serve.websocket.hub]: pairing state lock poisoned".to_string()
            })?;
            let pair = state.pairs.get(&pair_id).ok_or_else(|| {
                "error[serve.websocket.pairing]: paired session state disappeared".to_string()
            })?;
            [pair.first, pair.second].map(|id| {
                id.and_then(|id| state.sessions.get(&id))
                    .map(|session| session.outbound.clone())
            })
        };
        for (sender, payload) in senders.into_iter().zip([first, second]) {
            if let Some(sender) = sender {
                send_hub_payload(&sender, payload)?;
            }
        }
        Ok(())
    }

    fn broadcast_pair(&self, id: u64, payload: String) -> Result<(), String> {
        for sender in self.pair_senders(id)? {
            send_hub_payload(&sender, payload.clone())?;
        }
        Ok(())
    }

    fn pair_senders(&self, id: u64) -> Result<Vec<SyncSender<String>>, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "error[serve.websocket.hub]: pairing state lock poisoned".to_string())?;
        let session = state.sessions.get(&id).ok_or_else(|| {
            "error[serve.websocket.hub]: session is no longer registered".to_string()
        })?;
        let Some(pair_id) = session.pair else {
            return Ok(vec![session.outbound.clone()]);
        };
        let pair = state.pairs.get(&pair_id).ok_or_else(|| {
            "error[serve.websocket.pairing]: paired session state disappeared".to_string()
        })?;
        Ok([pair.first, pair.second]
            .into_iter()
            .flatten()
            .filter_map(|id| state.sessions.get(&id))
            .map(|session| session.outbound.clone())
            .collect())
    }

    fn transition_pair<F>(&self, id: u64, transition: F) -> Result<(), String>
    where
        F: FnOnce(String, i64, String, String) -> Result<(String, String, String), String>,
    {
        let deliveries = {
            let mut state = self.state.lock().map_err(|_| {
                "error[serve.websocket.hub]: pairing state lock poisoned".to_string()
            })?;
            let session = state.sessions.get(&id).ok_or_else(|| {
                "error[serve.websocket.hub]: session is no longer registered".to_string()
            })?;
            let pair_id = session.pair.ok_or_else(|| {
                "error[serve.websocket.pairing]: inbound frame arrived before a peer joined"
                    .to_string()
            })?;
            let role = session.role;
            let pair = state.pairs.get(&pair_id).ok_or_else(|| {
                "error[serve.websocket.pairing]: paired session state disappeared".to_string()
            })?;
            let (next, first_payload, second_payload) = transition(
                pair.state.clone(),
                role,
                pair.first_request.clone(),
                pair.second_request.clone(),
            )?;
            let pair = state
                .pairs
                .get_mut(&pair_id)
                .expect("validated pair remains locked");
            pair.state = next;
            [pair.first, pair.second]
                .into_iter()
                .zip([first_payload, second_payload])
                .filter_map(|(id, payload)| {
                    id.and_then(|id| state.sessions.get(&id))
                        .map(|session| (session.outbound.clone(), payload))
                })
                .collect::<Vec<_>>()
        };
        for (sender, payload) in deliveries {
            if !payload.is_empty() {
                send_hub_payload(&sender, payload)?;
            }
        }
        Ok(())
    }

    fn leave(&self, id: u64) {
        self.leave_at(id, Instant::now());
    }

    fn leave_at(&self, id: u64, now: Instant) {
        let notification = self.state.lock().ok().and_then(|mut state| {
            state.waiting.retain(|_, waiting_id| *waiting_id != id);
            let session = state.sessions.remove(&id)?;
            let pair_id = match session.pair {
                Some(pair_id) => pair_id,
                None => {
                    state.pairs.remove(&id);
                    return None;
                }
            };
            let pair = state.pairs.get_mut(&pair_id)?;
            if pair.first == Some(id) {
                pair.first = None;
            }
            if pair.second == Some(id) {
                pair.second = None;
            }
            let peer_id = pair.first.or(pair.second);
            let restorable = pair.retention.is_some();
            let route = pair.route.clone();
            let retained_room_capacity = pair.retained_room_capacity;
            if restorable && peer_id.is_none() {
                pair.expires_at = pair
                    .retention
                    .and_then(|retention| now.checked_add(retention));
            } else if !restorable {
                remove_pair(&mut state, pair_id);
                if let Some(peer_id) = peer_id {
                    if let Some(peer) = state.sessions.get_mut(&peer_id) {
                        peer.pair = None;
                    }
                }
            }
            if restorable && peer_id.is_none() {
                enforce_retained_capacity(&mut state, &route, retained_room_capacity);
            }
            peer_id.and_then(|peer_id| {
                state
                    .sessions
                    .get(&peer_id)
                    .map(|peer| (peer.outbound.clone(), peer.peer_left.clone()))
            })
        });
        if let Some((sender, payload)) = notification {
            let _ = send_hub_payload(&sender, payload);
        }
    }
}

fn reap_expired_rooms(state: &mut WebSocketHubState, now: Instant) {
    let expired = state
        .pairs
        .iter()
        .filter_map(|(pair_id, pair)| {
            pair.expires_at
                .filter(|expires_at| *expires_at <= now)
                .map(|_| *pair_id)
        })
        .collect::<Vec<_>>();
    for pair_id in expired {
        remove_pair(state, pair_id);
    }
}

fn enforce_retained_capacity(state: &mut WebSocketHubState, route: &str, capacity: usize) {
    let mut retained = state
        .pairs
        .iter()
        .filter_map(|(pair_id, pair)| {
            (pair.route == route && pair.first.is_none() && pair.second.is_none())
                .then_some((pair.expires_at, *pair_id))
        })
        .collect::<Vec<_>>();
    retained.sort_unstable();
    let excess = retained.len().saturating_sub(capacity);
    for (_, pair_id) in retained.into_iter().take(excess) {
        remove_pair(state, pair_id);
    }
}

fn remove_pair(state: &mut WebSocketHubState, pair_id: u64) {
    let Some(pair) = state.pairs.remove(&pair_id) else {
        return;
    };
    if let Some(room_id) = pair.room_id {
        state.rooms.remove(&(pair.route, room_id));
    }
}

impl WebSocketHubLease {
    pub(super) fn dispatch_admission(
        &mut self,
        session: &mut AotWebSocketCallbackSession,
    ) -> Result<(), String> {
        match self.admission.take() {
            Some(WebSocketHubAdmission::Matched {
                pair_id,
                room_id,
                first_request,
                second_request,
            }) => {
                let first = session.dispatch_pair_matched_output(
                    room_id.clone(),
                    1,
                    first_request.clone(),
                    second_request.clone(),
                )?;
                let second = session.dispatch_pair_matched_output(
                    room_id,
                    2,
                    first_request,
                    second_request,
                )?;
                self.hub.complete_match(pair_id, first, second)
            }
            Some(WebSocketHubAdmission::Restored {
                room_id,
                state,
                role,
                first_request,
                second_request,
            }) => {
                let payload = session.dispatch_pair_restored_output(
                    room_id,
                    state,
                    role,
                    first_request,
                    second_request,
                )?;
                self.hub.deliver(self.id, payload)
            }
            Some(WebSocketHubAdmission::Waiting) | None => Ok(()),
        }
    }

    pub(super) fn broadcast(&self, payload: String) -> Result<(), String> {
        self.hub.broadcast_pair(self.id, payload)
    }

    pub(super) fn transition<F>(&self, transition: F) -> Result<(), String>
    where
        F: FnOnce(String, i64, String, String) -> Result<(String, String, String), String>,
    {
        self.hub.transition_pair(self.id, transition)
    }

    pub(super) fn drain_stateful_inbound(
        &self,
        session: &mut AotWebSocketCallbackSession,
    ) -> Result<(), String> {
        loop {
            let mut dispatched = false;
            self.transition(|state, role, first_request, second_request| {
                let (did_dispatch, output) = session.dispatch_next_stateful_inbound_output(
                    state.clone(),
                    role,
                    first_request,
                    second_request,
                )?;
                dispatched = did_dispatch;
                if !did_dispatch {
                    return Ok((state, String::new(), String::new()));
                }
                stateful_transition_output(output)
            })?;
            if !dispatched {
                return Ok(());
            }
        }
    }
}

impl Drop for WebSocketHubLease {
    fn drop(&mut self) {
        self.hub.leave(self.id);
    }
}

fn restore_identity(
    request_target: &str,
    room_query: &str,
    player_query: &str,
) -> Result<Option<(String, String)>, String> {
    let Some((_, query)) = request_target.split_once('?') else {
        return Ok(None);
    };
    let parameters = form_urlencoded::parse(query.as_bytes()).collect::<HashMap<_, _>>();
    let room = parameters.get(room_query);
    let player = parameters.get(player_query);
    match (room, player) {
        (None, None) => Ok(None),
        (Some(room), Some(player)) if !room.is_empty() && !player.is_empty() => {
            Ok(Some((room.to_string(), player.to_string())))
        }
        _ => Err(
            "error[serve.websocket.restore]: room and player query values must be supplied together"
                .to_string(),
        ),
    }
}

fn send_hub_payload(sender: &SyncSender<String>, payload: String) -> Result<(), String> {
    sender.try_send(payload).map_err(|error| match error {
        TrySendError::Full(_) => {
            "error[serve.websocket.backpressure]: outbound session queue is full".to_string()
        }
        TrySendError::Disconnected(_) => {
            "error[serve.websocket.transport]: outbound session is disconnected".to_string()
        }
    })
}

fn stateful_transition_output(
    output: Option<ReplValue>,
) -> Result<(String, String, String), String> {
    match output {
        Some(ReplValue::Tuple(values)) => match values.as_slice() {
            [ReplValue::String(next), ReplValue::String(first), ReplValue::String(second)] => {
                Ok((next.clone(), first.clone(), second.clone()))
            }
            _ => Err("error[serve.websocket.callback_result]: stateful paired callback returned a tuple other than {String, String, String}".to_string()),
        },
        Some(value) => Err(format!(
            "error[serve.websocket.callback_result]: stateful paired callback returned {value:?}, expected {{String, String, String}}"
        )),
        None => Err("error[serve.websocket.callback_result]: stateful paired callback suspended without producing a transition".to_string()),
    }
}
