//! Blocking WebSocket scenarios used by integration-test flows.

use std::time::Duration;

use terlan_http_native::websocket::{client, Message};

use super::super::manifest_and_arguments::WebSocketCheck;

pub(in crate::commands::integration_test) fn run_websocket_check(
    host: &str,
    port: u16,
    check: &WebSocketCheck,
) -> Result<(), String> {
    let first_url = websocket_url(host, port, &check.first_path);
    let second_url = websocket_url(host, port, &check.second_path);
    let mut first_socket = client::connect(&first_url, Duration::from_secs(5))
        .map_err(|error| format!("cannot connect WebSocket {first_url}: {}", error.message()))?;
    let first_initial = next_websocket_text(&mut first_socket)?;
    require_websocket_contains(
        "first initial",
        &first_url,
        &first_initial,
        &check.first_initial_contains,
    )?;

    let mut second_socket = client::connect(&second_url, Duration::from_secs(5))
        .map_err(|error| format!("cannot connect WebSocket {second_url}: {}", error.message()))?;
    let second_match = next_websocket_text(&mut second_socket)?;
    let first_match = next_websocket_text(&mut first_socket)?;
    require_websocket_contains(
        "first match",
        &first_url,
        &first_match,
        &check.first_match_contains,
    )?;
    require_websocket_contains(
        "second match",
        &second_url,
        &second_match,
        &check.second_match_contains,
    )?;
    if let Some(move_check) = &check.move_check {
        let message = format!(
            r#"{{"type":"move","row":{},"column":{}}}"#,
            move_check.row, move_check.column
        );
        first_socket
            .send(Message::Text(message.into()))
            .map_err(|error| format!("cannot send WebSocket move to {first_url}: {error}"))?;
        let first_update = next_websocket_text(&mut first_socket)?;
        let second_update = next_websocket_text(&mut second_socket)?;
        require_websocket_contains(
            "first update",
            &first_url,
            &first_update,
            &move_check.first_update_contains,
        )?;
        require_websocket_contains(
            "second update",
            &second_url,
            &second_update,
            &move_check.second_update_contains,
        )?;
    }
    let mut restored_socket = None;
    if let Some(restore_check) = &check.restore_check {
        second_socket
            .close(None)
            .map_err(|error| format!("cannot close WebSocket {second_url}: {error}"))?;
        let _ = next_websocket_text(&mut first_socket)?;
        let restore_url = websocket_url(host, port, &restore_check.path);
        let mut socket =
            client::connect(&restore_url, Duration::from_secs(5)).map_err(|error| {
                format!(
                    "cannot restore WebSocket {restore_url}: {}",
                    error.message()
                )
            })?;
        let restored = next_websocket_text(&mut socket)?;
        require_websocket_contains(
            "restore entry",
            &restore_url,
            &restored,
            &restore_check.entry_contains,
        )?;
        require_websocket_contains(
            "restore view",
            &restore_url,
            &restored,
            &restore_check.view_contains,
        )?;
        restored_socket = Some(socket);
    }
    let _ = first_socket.close(None);
    let _ = second_socket.close(None);
    if let Some(mut socket) = restored_socket {
        let _ = socket.close(None);
    }

    if check.restore_check.is_some() {
        println!(
            "integration: WS PAIR_RESTORE {} + {} -> matched, moved, and restored",
            check.first_path, check.second_path
        );
    } else if check.move_check.is_some() {
        println!(
            "integration: WS PAIR_MOVE {} + {} -> matched and moved",
            check.first_path, check.second_path
        );
    } else {
        println!(
            "integration: WS PAIR {} + {} -> matched",
            check.first_path, check.second_path
        );
    }
    Ok(())
}

fn websocket_url(host: &str, port: u16, path: &str) -> String {
    format!("ws://{host}:{port}{path}")
}

fn next_websocket_text(socket: &mut client::Client) -> Result<String, String> {
    match socket.read() {
        Ok(Message::Text(text)) => Ok(text.to_string()),
        Ok(Message::Binary(bytes)) => String::from_utf8(bytes.to_vec())
            .map_err(|error| format!("WebSocket binary message was not UTF-8: {error}")),
        Ok(Message::Close(_)) => Err("WebSocket closed before expected message".to_string()),
        Ok(other) => Err(format!("unexpected WebSocket message: {other:?}")),
        Err(error) => Err(format!("cannot read WebSocket message: {error}")),
    }
}

fn require_websocket_contains(
    label: &str,
    url: &str,
    actual: &str,
    expected: &str,
) -> Result<(), String> {
    if actual.contains(expected) {
        return Ok(());
    }
    Err(format!(
        "WebSocket {label} message from {url} expected to contain `{expected}`, got `{actual}`"
    ))
}
