use serde_json::Value;
use tokio::sync::mpsc;

use super::*;

/// Verifies the first Battleship WebSocket player waits in the lobby.
///
/// Inputs:
/// - Empty WebSocket hub.
/// - One player query string with an encoded board.
///
/// Output:
/// - Test passes when the session has no room yet and receives a
///   `lobby_waiting` message.
///
/// Transformation:
/// - Exercises the local in-memory development WebSocket room state without
///   opening a network socket.
#[test]
fn opening_first_battleship_lobby_session_emits_waiting_message() {
    let hub = websocket_hub();
    let (tx, mut rx) = mpsc::unbounded_channel();

    let session = open_battleship_session(&hub, "player=Ada&board=%5B%5B%22X%22%5D%5D", tx)
        .expect("open lobby");

    assert_eq!(session.room_id, None);
    let message = rx.try_recv().expect("waiting message");
    let decoded = serde_json::from_str::<Value>(&message).expect("waiting json");
    assert_eq!(
        decoded.get("type").and_then(Value::as_str),
        Some("lobby_waiting")
    );
}

/// Verifies a waiting Battleship player joins the matched room state.
///
/// Inputs:
/// - Shared WebSocket hub.
/// - Two player query strings with encoded boards.
///
/// Output:
/// - Test passes when both players receive room updates after a move.
///
/// Transformation:
/// - Creates the first waiting session, matches it with a second player, then
///   applies a move through the same message handler used by the socket loop.
#[test]
fn waiting_lobby_session_can_move_after_match() {
    let hub = websocket_hub();
    let (first_tx, mut first_rx) = mpsc::unbounded_channel();
    let (second_tx, mut second_rx) = mpsc::unbounded_channel();
    let board = "board=%5B%5B%22X%22%5D%5D";

    let first = open_battleship_session(&hub, &format!("player=Ada&{board}"), first_tx)
        .expect("open first lobby");
    let _ = first_rx.try_recv().expect("waiting message");
    let second = open_battleship_session(&hub, &format!("player=Grace&{board}"), second_tx)
        .expect("open second lobby");
    assert!(first.room_id.is_none(), "first session started before room");
    assert!(second.room_id.is_some(), "second session creates room");

    let _ = first_rx.try_recv().expect("first match message");
    let _ = second_rx.try_recv().expect("second match message");
    handle_battleship_message(&hub, &first, r#"{"type":"move","row":0,"column":0}"#);

    let first_update =
        serde_json::from_str::<Value>(&first_rx.try_recv().expect("first room update"))
            .expect("first update json");
    let second_update =
        serde_json::from_str::<Value>(&second_rx.try_recv().expect("second room update"))
            .expect("second update json");
    assert_eq!(
        first_update.get("type").and_then(Value::as_str),
        Some("room_update")
    );
    assert_eq!(
        second_update.get("type").and_then(Value::as_str),
        Some("room_update")
    );
}

/// Verifies setup-board unit ids are treated as ship cells by the runtime.
///
/// Inputs:
/// - Two matched players, with the second player carrying a unit id at 0,0.
///
/// Output:
/// - Test passes when player one's move records a hit on player two's board.
///
/// Transformation:
/// - Guards the frontend/Terlan board contract where ships are serialized as
///   ids `0..9`, not display-only `X` cells.
#[test]
fn move_against_unit_id_records_hit_and_keeps_turn() {
    let hub = websocket_hub();
    let (first_tx, mut first_rx) = mpsc::unbounded_channel();
    let (second_tx, mut second_rx) = mpsc::unbounded_channel();
    let empty = "board=%5B%5B%220%22%5D%5D";
    let ship = "board=%5B%5B%220%22%2C%221%22%5D%5D";

    let first = open_battleship_session(&hub, &format!("player=Ada&{empty}"), first_tx)
        .expect("open first lobby");
    let _ = first_rx.try_recv().expect("waiting message");
    let _ = open_battleship_session(&hub, &format!("player=Grace&{ship}"), second_tx)
        .expect("open second lobby");
    let _ = first_rx.try_recv().expect("first match message");
    let _ = second_rx.try_recv().expect("second match message");

    handle_battleship_message(&hub, &first, r#"{"type":"move","row":0,"column":0}"#);

    let first_update =
        serde_json::from_str::<Value>(&first_rx.try_recv().expect("first room update"))
            .expect("first update json");
    let second_update =
        serde_json::from_str::<Value>(&second_rx.try_recv().expect("second room update"))
            .expect("second update json");
    assert_eq!(
        first_update
            .pointer("/view/opponent/board/0/0")
            .and_then(Value::as_str),
        Some("+")
    );
    assert_eq!(
        second_update
            .pointer("/view/own_player/board/0/0")
            .and_then(Value::as_str),
        Some("+")
    );
    assert_eq!(
        first_update
            .pointer("/view/allowed_actions/0/action")
            .and_then(Value::as_str),
        Some("move")
    );
    assert_eq!(
        first_update
            .pointer("/view/allowed_actions/0/target")
            .and_then(Value::as_str),
        Some("opponent_board")
    );
}

/// Verifies misses pass the turn to the opponent.
///
/// Inputs:
/// - Two matched players with an empty target cell.
///
/// Output:
/// - Test passes when the second player receives the next allowed move.
///
/// Transformation:
/// - Mirrors the migrated Terlan match rule that only hits retain the turn.
#[test]
fn move_against_empty_cell_records_miss_and_passes_turn() {
    let hub = websocket_hub();
    let (first_tx, mut first_rx) = mpsc::unbounded_channel();
    let (second_tx, mut second_rx) = mpsc::unbounded_channel();
    let first_board = "board=%5B%5B%220%22%5D%5D";
    let second_board = "board=%5B%5B%220%22%2C%22_%22%5D%5D";

    let first = open_battleship_session(&hub, &format!("player=Ada&{first_board}"), first_tx)
        .expect("open first lobby");
    let _ = first_rx.try_recv().expect("waiting message");
    let _ = open_battleship_session(&hub, &format!("player=Grace&{second_board}"), second_tx)
        .expect("open second lobby");
    let _ = first_rx.try_recv().expect("first match message");
    let _ = second_rx.try_recv().expect("second match message");

    handle_battleship_message(&hub, &first, r#"{"type":"move","row":0,"column":1}"#);

    let first_update =
        serde_json::from_str::<Value>(&first_rx.try_recv().expect("first room update"))
            .expect("first update json");
    let second_update =
        serde_json::from_str::<Value>(&second_rx.try_recv().expect("second room update"))
            .expect("second update json");
    assert_eq!(
        first_update
            .pointer("/view/opponent/board/0/1")
            .and_then(Value::as_str),
        Some("m")
    );
    assert!(first_update
        .pointer("/view/allowed_actions")
        .and_then(Value::as_array)
        .is_some_and(Vec::is_empty));
    assert_eq!(
        second_update
            .pointer("/view/allowed_actions/0/action")
            .and_then(Value::as_str),
        Some("move")
    );
}

/// Verifies resolved target cells are rejected.
///
/// Inputs:
/// - Two matched players with a miss already recorded on player two's board.
///
/// Output:
/// - Test passes when player one receives an `invalid_move` error and no room
///   update is broadcast.
///
/// Transformation:
/// - Restores the old room/rules behavior where repeated shots do not advance
///   the turn or mutate room state.
#[test]
fn move_against_resolved_cell_returns_invalid_move_error() {
    let hub = websocket_hub();
    let (first_tx, mut first_rx) = mpsc::unbounded_channel();
    let (second_tx, mut second_rx) = mpsc::unbounded_channel();
    let empty = "board=%5B%5B%22_%22%5D%5D";
    let resolved = "board=%5B%5B%22m%22%5D%5D";

    let first = open_battleship_session(&hub, &format!("player=Ada&{empty}"), first_tx)
        .expect("open first lobby");
    let _ = first_rx.try_recv().expect("waiting message");
    let _ = open_battleship_session(&hub, &format!("player=Grace&{resolved}"), second_tx)
        .expect("open second lobby");
    let _ = first_rx.try_recv().expect("first match message");
    let _ = second_rx.try_recv().expect("second match message");

    handle_battleship_message(&hub, &first, r#"{"type":"move","row":0,"column":0}"#);

    let error = serde_json::from_str::<Value>(&first_rx.try_recv().expect("invalid move error"))
        .expect("error json");
    assert_eq!(error.get("type").and_then(Value::as_str), Some("error"));
    assert_eq!(
        error.get("reason").and_then(Value::as_str),
        Some("invalid_move")
    );
    assert!(
        second_rx.try_recv().is_err(),
        "opponent should not receive update"
    );
}

/// Verifies the room phase finishes after the final unresolved ship is hit.
///
/// Inputs:
/// - A one-cell ship board for player two.
///
/// Output:
/// - Test passes when both room views report `finished`.
///
/// Transformation:
/// - Ensures the runtime uses the same remaining-ship detection as the Terlan
///   match rules.
#[test]
fn final_unit_id_hit_finishes_room() {
    let hub = websocket_hub();
    let (first_tx, mut first_rx) = mpsc::unbounded_channel();
    let (second_tx, mut second_rx) = mpsc::unbounded_channel();
    let empty = "board=%5B%5B%22_%22%5D%5D";
    let ship = "board=%5B%5B%220%22%5D%5D";

    let first = open_battleship_session(&hub, &format!("player=Ada&{empty}"), first_tx)
        .expect("open first lobby");
    let _ = first_rx.try_recv().expect("waiting message");
    let _ = open_battleship_session(&hub, &format!("player=Grace&{ship}"), second_tx)
        .expect("open second lobby");
    let _ = first_rx.try_recv().expect("first match message");
    let _ = second_rx.try_recv().expect("second match message");

    handle_battleship_message(&hub, &first, r#"{"type":"move","row":0,"column":0}"#);

    let first_update =
        serde_json::from_str::<Value>(&first_rx.try_recv().expect("first room update"))
            .expect("first update json");
    let second_update =
        serde_json::from_str::<Value>(&second_rx.try_recv().expect("second room update"))
            .expect("second update json");
    assert_eq!(
        first_update.pointer("/view/phase").and_then(Value::as_str),
        Some("finished")
    );
    assert_eq!(
        second_update.pointer("/view/phase").and_then(Value::as_str),
        Some("finished")
    );
    assert!(first_update
        .pointer("/view/allowed_actions")
        .and_then(Value::as_array)
        .is_some_and(Vec::is_empty));
}

/// Verifies lobby matchmaking follows the Terlan room contract IDs.
///
/// Inputs:
/// - Empty WebSocket hub.
/// - Two lobby players with encoded boards.
///
/// Output:
/// - Test passes when the first room is `room-1` and the players are
///   `player-1` and `player-2`.
///
/// Transformation:
/// - Exercises the local runtime allocation path that backs the
///   `battleship.RoomSession` contract exposed to the application.
#[test]
fn matching_lobby_sessions_assigns_contract_room_and_player_ids() {
    let hub = websocket_hub();
    let (first_tx, mut first_rx) = mpsc::unbounded_channel();
    let (second_tx, mut second_rx) = mpsc::unbounded_channel();
    let board = "board=%5B%5B%22X%22%5D%5D";

    let first = open_battleship_session(&hub, &format!("player=Ada&{board}"), first_tx)
        .expect("open first lobby");
    let _ = first_rx.try_recv().expect("waiting message");
    let second = open_battleship_session(&hub, &format!("player=Grace&{board}"), second_tx)
        .expect("open second lobby");

    let first_match =
        serde_json::from_str::<Value>(&first_rx.try_recv().expect("first match message"))
            .expect("first match json");
    let second_match =
        serde_json::from_str::<Value>(&second_rx.try_recv().expect("second match message"))
            .expect("second match json");

    assert_eq!(first.player_id, "player-1");
    assert_eq!(second.player_id, "player-2");
    assert_eq!(second.room_id.as_deref(), Some("room-1"));
    assert_eq!(
        first_match.get("type").and_then(Value::as_str),
        Some("match_found")
    );
    assert_eq!(
        first_match.get("room_id").and_then(Value::as_str),
        Some("room-1")
    );
    assert_eq!(
        first_match.get("player_id").and_then(Value::as_str),
        Some("player-1")
    );
    assert_eq!(
        first_match.get("opponent_id").and_then(Value::as_str),
        Some("player-2")
    );
    assert_eq!(
        second_match.get("type").and_then(Value::as_str),
        Some("match_found")
    );
    assert_eq!(
        second_match.get("room_id").and_then(Value::as_str),
        Some("room-1")
    );
    assert_eq!(
        second_match.get("player_id").and_then(Value::as_str),
        Some("player-2")
    );
    assert_eq!(
        second_match.get("opponent_id").and_then(Value::as_str),
        Some("player-1")
    );
}

/// Verifies a player can reconnect to an existing Battleship room.
///
/// Inputs:
/// - Matched room state.
/// - Restore query with the existing room and player ids.
///
/// Output:
/// - Test passes when the restored socket receives a `room_joined` message.
///
/// Transformation:
/// - Reattaches the outbound sender used by the WebSocket loop while keeping
///   the in-memory room state intact.
#[test]
fn restoring_existing_room_emits_room_joined_message() {
    let hub = websocket_hub();
    let (first_tx, mut first_rx) = mpsc::unbounded_channel();
    let (second_tx, mut second_rx) = mpsc::unbounded_channel();
    let board = "board=%5B%5B%22X%22%5D%5D";

    let _ = open_battleship_session(&hub, &format!("player=Ada&{board}"), first_tx)
        .expect("open first lobby");
    let _ = first_rx.try_recv().expect("waiting message");
    let _ = open_battleship_session(&hub, &format!("player=Grace&{board}"), second_tx)
        .expect("open second lobby");
    let _ = first_rx.try_recv().expect("first match message");
    let _ = second_rx.try_recv().expect("second match message");

    let (restore_tx, mut restore_rx) = mpsc::unbounded_channel();
    let restored = open_battleship_session(&hub, "room_id=room-1&player_id=player-1", restore_tx)
        .expect("restore room");
    let joined =
        serde_json::from_str::<Value>(&restore_rx.try_recv().expect("room joined message"))
            .expect("room joined json");

    assert_eq!(restored.room_id.as_deref(), Some("room-1"));
    assert_eq!(restored.player_id, "player-1");
    assert_eq!(
        joined.get("type").and_then(Value::as_str),
        Some("room_joined")
    );
    assert_eq!(
        joined.get("room_id").and_then(Value::as_str),
        Some("room-1")
    );
    assert_eq!(
        joined.get("player_id").and_then(Value::as_str),
        Some("player-1")
    );
    assert_eq!(
        joined.get("opponent_id").and_then(Value::as_str),
        Some("player-2")
    );
}

/// Verifies restoring a missing Battleship room returns a protocol error.
///
/// Inputs:
/// - Empty WebSocket hub.
/// - Restore query for a room that does not exist.
///
/// Output:
/// - Test passes when the returned JSON error reason is `room_not_found`.
///
/// Transformation:
/// - Exercises the error value sent to the browser before the socket closes.
#[test]
fn restoring_missing_room_returns_room_not_found_error() {
    let hub = websocket_hub();
    let (tx, _rx) = mpsc::unbounded_channel();

    let message = open_battleship_session(&hub, "room_id=missing&player_id=player-1", tx)
        .expect_err("missing room");
    let decoded = serde_json::from_str::<Value>(&message).expect("error json");

    assert_eq!(decoded.get("type").and_then(Value::as_str), Some("error"));
    assert_eq!(
        decoded.get("reason").and_then(Value::as_str),
        Some("room_not_found")
    );
}

/// Verifies restoring an unknown player in an existing room returns an error.
///
/// Inputs:
/// - Matched room state.
/// - Restore query for a player outside that room.
///
/// Output:
/// - Test passes when the returned JSON error reason is `unknown_player`.
///
/// Transformation:
/// - Covers the browser-facing reconnect failure branch for stale player ids.
#[test]
fn restoring_unknown_player_returns_unknown_player_error() {
    let hub = websocket_hub();
    let (first_tx, mut first_rx) = mpsc::unbounded_channel();
    let (second_tx, mut second_rx) = mpsc::unbounded_channel();
    let board = "board=%5B%5B%22X%22%5D%5D";

    let _ = open_battleship_session(&hub, &format!("player=Ada&{board}"), first_tx)
        .expect("open first lobby");
    let _ = first_rx.try_recv().expect("waiting message");
    let _ = open_battleship_session(&hub, &format!("player=Grace&{board}"), second_tx)
        .expect("open second lobby");
    let _ = first_rx.try_recv().expect("first match message");
    let _ = second_rx.try_recv().expect("second match message");

    let (restore_tx, _restore_rx) = mpsc::unbounded_channel();
    let message = open_battleship_session(&hub, "room_id=room-1&player_id=player-9", restore_tx)
        .expect_err("unknown player");
    let decoded = serde_json::from_str::<Value>(&message).expect("error json");

    assert_eq!(decoded.get("type").and_then(Value::as_str), Some("error"));
    assert_eq!(
        decoded.get("reason").and_then(Value::as_str),
        Some("unknown_player")
    );
}
