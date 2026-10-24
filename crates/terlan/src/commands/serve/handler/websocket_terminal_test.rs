//! Source terminal callbacks run through the package's asynchronous lifecycle.

use super::super::protocol_test_support::complete;
use super::*;
use terlan_http_native::websocket::connection::Callbacks;

const SOURCE: &str = r#"module app.SocketCallbacks.
import std.http.{Router, Sse, WebSocket}.
import std.vm.Process.
import type std.http.Router.
pub opened(): Unit -> Unit.
pub inbound(_frame: String): Unit -> Unit.
pub writable(): Unit -> Unit.
pub cancelled(_reason: String): Unit -> Unit.
pub closed(): Unit -> Process.sleep(Process.timer(1)).
pub failed_closed(): Unit ->
    Process.sleep(Process.timer(1));
    let _invalid = Sse.response([Sse.data("never emitted").with_name("bad\nname")]);
    Unit.
pub router(): Router -> Router.new().websocket("/socket",
    WebSocket.endpoint(4, 1024).callbacks(opened, inbound, writable, closed, cancelled)).
"#;

#[test]
fn compiled_websocket_close_awaits_source_timer_and_retains_failure_without_reentry() {
    let (root, runtime, plan) = super::websocket_invocation_test::runtime_with_source(SOURCE);
    for failure in [false, true] {
        let mut callbacks = plan.callbacks().unwrap().clone();
        if failure {
            callbacks.close = crate::runtime::vm::native_callable::VmNativeCallableRef {
                module: "app.SocketCallbacks".into(),
                function: "failed_closed".into(),
                arity: 0,
            }
            .into_value();
        }
        let plan = WebSocketEndpointPlan::new(4, 1024)
            .unwrap()
            .with_callbacks(callbacks)
            .unwrap();
        let mut session = open(
            runtime.clone(),
            "app.SocketCallbacks".into(),
            Session::open(plan),
        )
        .unwrap();
        let result = complete(Callbacks::close(&mut session));
        if failure {
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("Close callback failed"));
        } else {
            result.unwrap();
            assert!(session
                .executor()
                .completed_events()
                .contains(&AotWebSocketCallbackEvent::Close));
        }
        assert!(!session.is_open());
        assert!(!session.is_waiting());
        let completed = session.executor().completed_events().to_vec();
        complete(Callbacks::close(&mut session)).unwrap();
        assert_eq!(session.executor().completed_events(), completed);
        assert!(!completed.contains(&AotWebSocketCallbackEvent::Cancellation));
    }
    std::fs::remove_dir_all(root).unwrap();
}
