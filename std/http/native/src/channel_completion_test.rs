use super::*;
use crate::callback_test_support::{Executor, State, Wait};
use crate::channel_plan::{SseEvent, WebSocketEvent};
use terlan_runtime_abi::NativeValue;

fn terminal_contract<E: Copy + Debug + Send>(event: E, channel: &str) {
    let mut invocation = Executor::<E>::default();
    let value = NativeValue::String("source result".into());
    let State::Complete(actual) = finish_terminal(
        &mut invocation,
        event,
        State::Complete(value.clone()),
        channel,
    )
    .unwrap() else {
        panic!("completed callback changed state");
    };
    assert_eq!(actual, value);
    assert!(invocation.cancellations.is_empty());

    for wait in [Wait::Text, Wait::Bytes] {
        for fail in [false, true] {
            let mut invocation = Executor::<E> {
                pending: Some(wait),
                cancel_error: fail,
                ..Executor::default()
            };
            let error = finish_terminal(&mut invocation, event, State::Waiting(wait), channel)
                .unwrap_err()
                .to_string();
            assert!(
                error.starts_with(&format!("error[serve.{channel}.terminal_wait]:")),
                "{error}"
            );
            assert_eq!(error.contains("cancellation failed: cancel failed"), fail);
            assert_eq!(
                invocation.cancellations,
                [format!("terminal {event:?} callback cannot suspend")]
            );
            assert!(!invocation.is_waiting());
            assert!(invocation.calls.is_empty());
        }
    }
}

#[test]
fn websocket_terminal_policy_is_package_owned_for_close_and_cancellation() {
    for event in [WebSocketEvent::Close, WebSocketEvent::Cancellation] {
        terminal_contract(event, "websocket");
    }
}

#[test]
fn sse_terminal_policy_is_package_owned_for_drain_and_cancellation() {
    for event in [SseEvent::Drain, SseEvent::Cancellation] {
        terminal_contract(event, "sse");
    }
}
