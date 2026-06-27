use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use http_body_util::{BodyExt, Full};
use hyper::upgrade::OnUpgrade;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::protocol::{Message, Role};
use tokio_tungstenite::WebSocketStream;

use super::handler::WebPackageWebSocket;
use super::manifest::read_web_manifest;
use super::ServeBody;

/// Shared WebSocket room state for one local `terlc serve` runtime.
///
/// Inputs:
/// - Created once per bound server and cloned into connection tasks.
///
/// Output:
/// - In-memory lobby and room registry for the currently served package.
///
/// Transformation:
/// - Gives local development a long-lived state boundary without changing the
///   existing one-shot BEAM HTTP handler ABI.
pub(super) type WebSocketHub = Arc<Mutex<WebSocketState>>;

/// Battleship board represented as rows of cell markers.
///
/// Inputs:
/// - JSON board values supplied by browser clients.
///
/// Output:
/// - Mutable in-memory board used by the local development WebSocket room.
///
/// Transformation:
/// - Keeps browser payloads as simple string grids so room updates can be
///   serialized without a separate game-domain type.
type Board = Vec<Vec<String>>;

/// Mutable state for local WebSocket rooms.
///
/// Inputs:
/// - Player joins, reconnects, moves, and disconnects.
///
/// Output:
/// - Waiting lobby player plus active room map.
///
/// Transformation:
/// - Stores development-only room state behind `WebSocketHub` so each serve
///   runtime has a single coordination boundary.
#[derive(Debug)]
pub(super) struct WebSocketState {
    waiting: Option<RoomPlayer>,
    rooms: HashMap<String, Room>,
    next_room_sequence: u64,
    next_player_sequence: u64,
}

impl Default for WebSocketState {
    /// Creates empty development WebSocket room state.
    ///
    /// Inputs:
    /// - None.
    ///
    /// Output:
    /// - State with no waiting player, no rooms, and deterministic counters.
    ///
    /// Transformation:
    /// - Initializes runtime bookkeeping without opening sockets or spawning
    ///   background tasks.
    fn default() -> Self {
        Self {
            waiting: None,
            rooms: HashMap::new(),
            next_room_sequence: 1,
            next_player_sequence: 1,
        }
    }
}

/// One active Battleship room.
///
/// Inputs:
/// - Two matched lobby players.
///
/// Output:
/// - Room state used to render per-player views and route moves.
///
/// Transformation:
/// - Tracks turn ownership and phase separately from player boards so updates
///   can be broadcast as source-neutral JSON messages.
#[derive(Debug)]
struct Room {
    id: String,
    players: HashMap<String, RoomPlayer>,
    turn_player_id: String,
    phase: String,
}

/// One WebSocket player connected to a room or lobby.
///
/// Inputs:
/// - Player name, board payload, and outbound message channel.
///
/// Output:
/// - Per-player room state.
///
/// Transformation:
/// - Stores the sender as optional so disconnects can keep room history while
///   preventing writes to a closed socket.
#[derive(Debug, Clone)]
struct RoomPlayer {
    id: String,
    name: String,
    board: Board,
    tx: Option<mpsc::UnboundedSender<String>>,
}

/// Builds a fresh WebSocket hub for one serve runtime.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Empty shared WebSocket state.
///
/// Transformation:
/// - Wraps default room state in `Arc<Mutex<_>>` for per-connection tasks.
pub(super) fn websocket_hub() -> WebSocketHub {
    Arc::new(Mutex::new(WebSocketState::default()))
}

/// Finds a manifest WebSocket route for an incoming request.
///
/// Inputs:
/// - `web_root`: package root containing `manifest.json`.
/// - `method`: request method.
/// - `request_path`: URL path without query text.
///
/// Output:
/// - Matching WebSocket manifest entry, if any.
///
/// Transformation:
/// - Keeps WebSocket route discovery manifest-owned while the first supported
///   route class is exact-path matching for the Battleship `/ws` endpoint.
pub(super) fn manifest_websocket_for_request(
    web_root: &Path,
    method: &str,
    request_path: &str,
) -> Option<WebPackageWebSocket> {
    if method != "GET" {
        return None;
    }
    read_web_manifest(web_root)
        .ok()?
        .websockets
        .into_iter()
        .find(|websocket| websocket.route == request_path)
}

/// Returns whether the request headers ask for a WebSocket upgrade.
///
/// Inputs:
/// - HTTP headers from an incoming request.
///
/// Output:
/// - `true` when the required WebSocket upgrade headers are present.
///
/// Transformation:
/// - Applies the minimal header checks needed before Hyper upgrades the
///   request body.
pub(super) fn is_websocket_upgrade(headers: &http::HeaderMap) -> bool {
    let upgrade = headers
        .get(http::header::UPGRADE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("websocket"));
    let connection = headers
        .get(http::header::CONNECTION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case("upgrade"))
        });
    headers.contains_key("sec-websocket-key") && upgrade && connection
}

/// Builds the HTTP 101 switching-protocols response for a WebSocket request.
///
/// Inputs:
/// - Original HTTP request containing `sec-websocket-key`.
///
/// Output:
/// - HTTP 101 response with WebSocket upgrade headers.
///
/// Transformation:
/// - Derives the accept key through tungstenite and returns an empty Hyper
///   response body for the protocol switch.
pub(super) fn websocket_upgrade_response<B>(request: &Request<B>) -> Response<ServeBody> {
    let accept_key = request
        .headers()
        .get("sec-websocket-key")
        .and_then(|value| value.to_str().ok())
        .map(|value| derive_accept_key(value.as_bytes()))
        .unwrap_or_default();
    let body = Full::new(Bytes::new()).boxed();
    Response::builder()
        .status(http::StatusCode::SWITCHING_PROTOCOLS)
        .header(http::header::UPGRADE, "websocket")
        .header(http::header::CONNECTION, "Upgrade")
        .header("sec-websocket-accept", accept_key)
        .body(body)
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::new()).boxed()))
}

/// Runs one upgraded WebSocket connection.
///
/// Inputs:
/// - `upgrade`: Hyper upgrade future for the accepted request.
/// - `hub`: shared room state.
/// - `websocket`: manifest route metadata.
/// - `query`: request query string.
///
/// Output:
/// - None; errors are logged and close the connection.
///
/// Transformation:
/// - Converts Hyper's upgraded socket to a tungstenite stream and dispatches
///   the first supported runtime protocol, `battleship.room.v1`.
pub(super) async fn serve_websocket_upgrade(
    upgrade: OnUpgrade,
    hub: WebSocketHub,
    websocket: WebPackageWebSocket,
    query: String,
) {
    let upgraded = match upgrade.await {
        Ok(upgraded) => upgraded,
        Err(err) => {
            eprintln!("error[serve_websocket]: websocket upgrade failed: {err}");
            return;
        }
    };
    let stream = WebSocketStream::from_raw_socket(TokioIo::new(upgraded), Role::Server, None).await;
    if websocket.protocol == "battleship.room.v1" {
        serve_battleship_room_socket(stream, hub, &query).await;
    }
}

/// Serves one Battleship room WebSocket stream.
///
/// Inputs:
/// - Upgraded tungstenite stream.
/// - Shared room hub.
/// - Query string carrying player, board, or reconnect metadata.
///
/// Output:
/// - None; the async task ends when the socket closes or errors.
///
/// Transformation:
/// - Opens/restores a room session, forwards outbound room messages to the
///   socket, handles inbound move frames, and unregisters the session on close.
async fn serve_battleship_room_socket(
    stream: WebSocketStream<TokioIo<hyper::upgrade::Upgraded>>,
    hub: WebSocketHub,
    query: &str,
) {
    let (mut ws_tx, mut ws_rx) = stream.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();
    let session = match open_battleship_session(&hub, query, out_tx.clone()) {
        Ok(session) => session,
        Err(message) => {
            let _ = ws_tx.send(Message::Text(message.into())).await;
            let _ = ws_tx.close().await;
            return;
        }
    };

    loop {
        tokio::select! {
            outbound = out_rx.recv() => {
                let Some(outbound) = outbound else {
                    break;
                };
                if ws_tx.send(Message::Text(outbound.into())).await.is_err() {
                    break;
                }
            }
            inbound = ws_rx.next() => {
                match inbound {
                    Some(Ok(Message::Text(text))) => handle_battleship_message(&hub, &session, text.as_str()),
                    Some(Ok(Message::Binary(bytes))) => {
                        if let Ok(text) = std::str::from_utf8(&bytes) {
                            handle_battleship_message(&hub, &session, text);
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(err)) => {
                        eprintln!("error[serve_websocket]: websocket frame failed: {err}");
                        break;
                    }
                }
            }
        }
    }
    close_battleship_session(&hub, &session);
}

/// Active socket session identity.
///
/// Inputs:
/// - Room/lobby join result.
///
/// Output:
/// - Player id and optional room id used by message and close handlers.
///
/// Transformation:
/// - Keeps connection-local identity small so reconnect state remains owned by
///   the shared hub.
#[derive(Debug, Clone)]
struct SocketSession {
    room_id: Option<String>,
    player_id: String,
}

/// Opens or restores one Battleship room session.
///
/// Inputs:
/// - Shared room hub.
/// - Query string containing lobby or reconnect parameters.
/// - Outbound channel for messages to this socket.
///
/// Output:
/// - Session identity or JSON-encoded error message.
///
/// Transformation:
/// - Parses query params, decodes a board when supplied, restores existing room
///   membership when ids are present, or joins the lobby.
fn open_battleship_session(
    hub: &WebSocketHub,
    query: &str,
    tx: mpsc::UnboundedSender<String>,
) -> Result<SocketSession, String> {
    let params = query_params(query);
    if let (Some(room_id), Some(player_id)) = (params.get("room_id"), params.get("player_id")) {
        return restore_battleship_room(hub, room_id, player_id, tx);
    }
    let player_name = params
        .get("player")
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| "Anonymous".to_string());
    let board = params
        .get("board")
        .and_then(|value| serde_json::from_str::<Board>(value).ok())
        .unwrap_or_else(empty_board);
    join_battleship_lobby(hub, player_name, board, tx)
}

/// Joins the Battleship lobby and possibly creates a room.
///
/// Inputs:
/// - Shared room hub.
/// - Player name, board, and outbound channel.
///
/// Output:
/// - Session identity for the joined player or JSON-encoded error message.
///
/// Transformation:
/// - Stores the first player as waiting; when a second player arrives, creates
///   a room and sends both clients an initial `match_found` message.
fn join_battleship_lobby(
    hub: &WebSocketHub,
    name: String,
    board: Board,
    tx: mpsc::UnboundedSender<String>,
) -> Result<SocketSession, String> {
    let mut state = hub
        .lock()
        .map_err(|_| error_message("server_error", "room_state_lock"))?;
    let player = RoomPlayer {
        id: next_player_id(&mut state),
        name,
        board,
        tx: Some(tx),
    };
    let session = SocketSession {
        room_id: None,
        player_id: player.id.clone(),
    };
    let Some(waiting) = state.waiting.take() else {
        send_to_player(&player, &json!({ "type": "lobby_waiting" }));
        state.waiting = Some(player);
        return Ok(session);
    };

    let room_id = next_room_id(&mut state);
    let turn_player_id = waiting.id.clone();
    let current_player_id = player.id.clone();
    let mut room = Room {
        id: room_id.clone(),
        players: HashMap::new(),
        turn_player_id,
        phase: "playing".to_string(),
    };
    room.players.insert(waiting.id.clone(), waiting);
    room.players.insert(player.id.clone(), player);
    send_room_entry(&room, &current_player_id, "match_found");
    if let Some(other_id) = room
        .players
        .keys()
        .find(|candidate| *candidate != &current_player_id)
        .cloned()
    {
        send_room_entry(&room, &other_id, "match_found");
    }
    state.rooms.insert(room_id.clone(), room);
    Ok(SocketSession {
        room_id: Some(room_id),
        player_id: current_player_id,
    })
}

/// Restores one player connection to an existing room.
///
/// Inputs:
/// - Shared room hub.
/// - Room id, player id, and replacement outbound channel.
///
/// Output:
/// - Session identity or JSON-encoded error message.
///
/// Transformation:
/// - Reattaches the player's sender and emits a fresh room view.
fn restore_battleship_room(
    hub: &WebSocketHub,
    room_id: &str,
    player_id: &str,
    tx: mpsc::UnboundedSender<String>,
) -> Result<SocketSession, String> {
    let mut state = hub
        .lock()
        .map_err(|_| error_message("server_error", "room_state_lock"))?;
    let Some(room) = state.rooms.get_mut(room_id) else {
        return Err(error_message("room_not_found", "room_not_found"));
    };
    let Some(player) = room.players.get_mut(player_id) else {
        return Err(error_message("unknown_player", "unknown_player"));
    };
    player.tx = Some(tx);
    send_room_entry(room, player_id, "room_joined");
    Ok(SocketSession {
        room_id: Some(room_id.to_string()),
        player_id: player_id.to_string(),
    })
}

/// Handles one inbound Battleship WebSocket message.
///
/// Inputs:
/// - Shared room hub.
/// - Connection session identity.
/// - Text frame payload.
///
/// Output:
/// - None; invalid or out-of-turn messages are ignored.
///
/// Transformation:
/// - Decodes move messages, mutates the opponent board, advances the turn, and
///   broadcasts updated room views.
fn handle_battleship_message(hub: &WebSocketHub, session: &SocketSession, text: &str) {
    let Ok(message) = serde_json::from_str::<Value>(text) else {
        return;
    };
    if message.get("type").and_then(Value::as_str) != Some("move") {
        return;
    }
    let Some(row) = message
        .get("row")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
    else {
        return;
    };
    let Some(column) = message
        .get("column")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
    else {
        return;
    };
    let Ok(mut state) = hub.lock() else {
        return;
    };
    let room_id = session
        .room_id
        .clone()
        .or_else(|| room_id_for_player(&state, &session.player_id));
    let Some(room_id) = room_id else {
        return;
    };
    let Some(room) = state.rooms.get_mut(&room_id) else {
        return;
    };
    if room.turn_player_id != session.player_id {
        return;
    }
    let Some(opponent_id) = opponent_id(room, &session.player_id) else {
        return;
    };
    let move_result = if let Some(opponent) = room.players.get_mut(&opponent_id) {
        apply_move_to_board(&mut opponent.board, row, column)
    } else {
        MoveResult::Invalid
    };
    match move_result {
        MoveResult::Hit => {}
        MoveResult::Miss => {
            room.turn_player_id = opponent_id;
        }
        MoveResult::Invalid => {
            if let Some(player) = room.players.get(&session.player_id) {
                send_to_player(
                    player,
                    &json!({ "type": "error", "reason": "invalid_move" }),
                );
            }
            return;
        }
    }
    if room
        .players
        .values()
        .any(|player| remaining_units(&player.board) == 0)
    {
        room.phase = "finished".to_string();
    }
    broadcast_room_update(room);
}

/// Closes one Battleship room session.
///
/// Inputs:
/// - Shared room hub.
/// - Connection session identity.
///
/// Output:
/// - None.
///
/// Transformation:
/// - Removes waiting lobby state or clears the player's sender while notifying
///   the opponent that the socket left.
fn close_battleship_session(hub: &WebSocketHub, session: &SocketSession) {
    let Ok(mut state) = hub.lock() else {
        return;
    };
    if let Some(waiting) = state.waiting.as_ref() {
        if waiting.id == session.player_id {
            state.waiting = None;
            return;
        }
    }
    let room_id = session
        .room_id
        .clone()
        .or_else(|| room_id_for_player(&state, &session.player_id));
    let Some(room_id) = room_id else {
        return;
    };
    let Some(room) = state.rooms.get_mut(&room_id) else {
        return;
    };
    if let Some(player) = room.players.get_mut(&session.player_id) {
        player.tx = None;
    }
    if let Some(opponent_id) = opponent_id(room, &session.player_id) {
        if let Some(opponent) = room.players.get(&opponent_id) {
            send_to_player(opponent, &json!({ "type": "opponent_left" }));
        }
    }
}

/// Broadcasts current room views to all connected room players.
///
/// Inputs:
/// - Room state.
///
/// Output:
/// - None.
///
/// Transformation:
/// - Builds each player's perspective and sends a `room_update` message through
///   the player's optional outbound channel.
fn broadcast_room_update(room: &Room) {
    let player_ids = room.players.keys().cloned().collect::<Vec<_>>();
    for player_id in player_ids {
        if let Some(view) = room_view(room, &player_id) {
            if let Some(player) = room.players.get(&player_id) {
                send_to_player(player, &json!({ "type": "room_update", "view": view }));
            }
        }
    }
}

/// Sends an initial room entry message to one player.
///
/// Inputs:
/// - Room state.
/// - Recipient player id.
/// - Message type such as `match_found` or `room_joined`.
///
/// Output:
/// - None.
///
/// Transformation:
/// - Combines room ids, opponent ids, and player-specific room view into a
///   single JSON message.
fn send_room_entry(room: &Room, player_id: &str, message_type: &str) {
    let Some(player) = room.players.get(player_id) else {
        return;
    };
    let opponent = opponent_id(room, player_id);
    let view = room_view(room, player_id).unwrap_or_else(|| json!({}));
    send_to_player(
        player,
        &json!({
            "type": message_type,
            "room_id": room.id,
            "player_id": player_id,
            "opponent_id": opponent,
            "view": view
        }),
    );
}

/// Builds one player's room view.
///
/// Inputs:
/// - Room state.
/// - Player id for the requested perspective.
///
/// Output:
/// - JSON view or `None` if the player/opponent cannot be found.
///
/// Transformation:
/// - Exposes the player's own board, a redacted opponent board, and allowed
///   actions based on room phase and turn ownership.
fn room_view(room: &Room, player_id: &str) -> Option<Value> {
    let own = room.players.get(player_id)?;
    let opponent_id = opponent_id(room, player_id)?;
    let opponent = room.players.get(&opponent_id)?;
    let allowed_actions = if room.phase != "finished" && room.turn_player_id == player_id {
        json!([{ "action": "move", "target": "opponent_board" }])
    } else {
        json!([])
    };
    Some(json!({
        "phase": room.phase,
        "own_player": {
            "id": own.id,
            "name": own.name,
            "board": own.board
        },
        "opponent": {
            "id": opponent.id,
            "name": opponent.name,
            "board": public_target_board(&opponent.board)
        },
        "allowed_actions": allowed_actions
    }))
}

/// Sends a JSON message to a connected player.
///
/// Inputs:
/// - Room player.
/// - JSON value to send.
///
/// Output:
/// - None.
///
/// Transformation:
/// - Serializes the JSON value and drops send errors because disconnect cleanup
///   is handled by the socket task.
fn send_to_player(player: &RoomPlayer, value: &Value) {
    if let Some(tx) = &player.tx {
        let _ = tx.send(value.to_string());
    }
}

/// Parses query parameters into owned strings.
///
/// Inputs:
/// - Raw URL query string without the leading `?`.
///
/// Output:
/// - Map of decoded query keys to decoded values.
///
/// Transformation:
/// - Delegates percent-decoding to `url::form_urlencoded`.
fn query_params(query: &str) -> HashMap<String, String> {
    url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect()
}

/// Builds the default empty Battleship board.
///
/// Inputs:
/// - None.
///
/// Output:
/// - 10x10 board with unknown cells.
///
/// Transformation:
/// - Produces a JSON-friendly grid used when clients do not provide a board.
fn empty_board() -> Board {
    vec![vec!["_".to_string(); 10]; 10]
}

/// Redacts a board for opponent viewing.
///
/// Inputs:
/// - Full player board.
///
/// Output:
/// - Board containing only known hits and misses.
///
/// Transformation:
/// - Replaces ship and unknown cells with `_` while preserving `+` and `m`.
fn public_target_board(board: &Board) -> Board {
    board
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| match cell.as_str() {
                    "+" => "+".to_string(),
                    "m" => "m".to_string(),
                    _ => "_".to_string(),
                })
                .collect()
        })
        .collect()
}

/// Result of applying a move to a Battleship board.
///
/// Inputs:
/// - One target cell classification.
///
/// Output:
/// - Hit, miss, or invalid move marker.
///
/// Transformation:
/// - Keeps move-result branching explicit so turn advancement mirrors the
///   Terlan `battleship.rules.Match` contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MoveResult {
    Hit,
    Miss,
    Invalid,
}

/// Applies a move to a board.
///
/// Inputs:
/// - Mutable board.
/// - Row and column index.
///
/// Output:
/// - Move result used for turn and error handling.
///
/// Transformation:
/// - Converts ship cells to hits, empty cells to misses, and rejects already
///   resolved or out-of-bounds cells.
fn apply_move_to_board(board: &mut Board, row: usize, column: usize) -> MoveResult {
    let Some(cells) = board.get_mut(row) else {
        return MoveResult::Invalid;
    };
    let Some(cell) = cells.get_mut(column) else {
        return MoveResult::Invalid;
    };
    if is_ship_cell(cell) {
        *cell = "+".to_string();
        return MoveResult::Hit;
    }
    match cell.as_str() {
        "_" => {
            *cell = "m".to_string();
            MoveResult::Miss
        }
        _ => MoveResult::Invalid,
    }
}

/// Counts remaining ship cells.
///
/// Inputs:
/// - Player board.
///
/// Output:
/// - Number of unresolved ship cells.
///
/// Transformation:
/// - Counts cells containing frontend unit ids. `X` remains accepted for
///   compatibility with earlier smoke fixtures.
fn remaining_units(board: &Board) -> usize {
    board
        .iter()
        .flatten()
        .filter(|cell| is_ship_cell(cell))
        .count()
}

/// Returns whether a cell represents an unresolved ship.
///
/// Inputs:
/// - One serialized board cell.
///
/// Output:
/// - `true` for the setup-board unit ids `0..9` and legacy `X` fixture cells.
///
/// Transformation:
/// - Keeps the Rust development runtime aligned with the Terlan and frontend
///   board contracts.
fn is_ship_cell(cell: &str) -> bool {
    matches!(
        cell,
        "X" | "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"
    )
}

/// Finds the opponent id for a player in a two-player room.
///
/// Inputs:
/// - Room state.
/// - Current player id.
///
/// Output:
/// - Opponent id, if present.
///
/// Transformation:
/// - Selects the first room player whose id differs from the current player.
fn opponent_id(room: &Room, player_id: &str) -> Option<String> {
    room.players
        .keys()
        .find(|candidate| candidate.as_str() != player_id)
        .cloned()
}

/// Finds the room id containing a player.
///
/// Inputs:
/// - Shared room state.
/// - Player id.
///
/// Output:
/// - Room id, if the player belongs to an active room.
///
/// Transformation:
/// - Scans active rooms by membership.
fn room_id_for_player(state: &WebSocketState, player_id: &str) -> Option<String> {
    state
        .rooms
        .iter()
        .find(|(_, room)| room.players.contains_key(player_id))
        .map(|(room_id, _)| room_id.clone())
}

/// Allocates the next development room id.
///
/// Inputs:
/// - Mutable hub state.
///
/// Output:
/// - Stable room id string for the current serve runtime.
///
/// Transformation:
/// - Prefixes the hub-local sequence with `room-`.
fn next_room_id(state: &mut WebSocketState) -> String {
    let id = state.next_room_sequence;
    state.next_room_sequence += 1;
    format!("room-{id}")
}

/// Allocates the next development player id.
///
/// Inputs:
/// - Mutable hub state.
///
/// Output:
/// - Stable player id string for the current serve runtime.
///
/// Transformation:
/// - Prefixes the hub-local sequence with `player-`.
fn next_player_id(state: &mut WebSocketState) -> String {
    let id = state.next_player_sequence;
    state.next_player_sequence += 1;
    format!("player-{id}")
}

/// Builds a JSON error message.
///
/// Inputs:
/// - Error kind.
/// - Human-readable reason, or empty string to reuse the kind.
///
/// Output:
/// - Serialized JSON error message.
///
/// Transformation:
/// - Normalizes empty reasons and emits the browser protocol error shape.
fn error_message(kind: &str, reason: &str) -> String {
    let reason = if reason.is_empty() { kind } else { reason };
    json!({ "type": "error", "reason": reason }).to_string()
}

#[cfg(test)]
#[path = "websocket_test.rs"]
mod websocket_test;
