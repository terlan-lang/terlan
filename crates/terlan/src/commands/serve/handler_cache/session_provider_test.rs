//! Canonical session declarations execute through their source provider.

use super::super::{AotHandlerGeneration, AotHandlerRuntime};
use super::compile_native_handler_fixture;
use crate::runtime::vm::http_session::{self, VmHttpSessionRuntime, VmHttpSessionService};
use crate::runtime::vm::ReplValue;
use std::collections::HashMap;
use std::sync::Arc;

#[test]
fn canonical_session_provider_preserves_commands_rotation_and_expiration() {
    let fixture = compile_native_handler_fixture(
        "source_session_provider",
        "src/app/Sessions.terl",
        "app_Sessions",
        r#"module app.Sessions.
import std.http.Session.
import std.core.Unit.
import std.core.Option.{with_default, Some, None}.
pub update(session: Session): Bool ->
    let inserted = session.set("key", "value");
    let found = with_default(session.get("key"), "missing") == "value"
        and (case session.get("key") {
            Some("wrong") -> false;
            Some(value) where value == "wrong" -> false;
            Some("value") -> true;
            _ -> false
        });
    let deleted = session.delete("key");
    let missing = with_default(session.get("key"), "missing") == "missing"
        and (case session.get("key") { Some(_) -> false; None -> true });
    session.set("key", "");
    let empty = case session.get("key") { Some("") -> true; _ -> false };
    session.set("kept", "retained");
    inserted == Unit and deleted == Unit and found and missing and empty.
pub rotate(session: Session): Session -> session.rotate().
pub read(session: Session): String -> with_default(session.get("kept"), "missing").
pub expire(session: Session): Bool ->
    let done = session.expire();
    done == Unit.
"#,
    );
    let mut state = VmHttpSessionRuntime::new("source-session-test", 100).unwrap();
    let (handle, cookie) = http_session::current(&mut state, None)
        .unwrap()
        .into_managed_parts();
    let session = ReplValue::Record {
        name: "Session".into(),
        fields: vec![
            (
                "identity".into(),
                ReplValue::String(handle.managed_id().into()),
            ),
            (
                "pending_identity".into(),
                ReplValue::String(cookie.unwrap()),
            ),
        ],
    };
    let sessions = VmHttpSessionService::new(state);
    let runtime = AotHandlerRuntime {
        module: "app.Sessions".into(),
        generation: Arc::new(
            AotHandlerGeneration::load_with_shard_count(&fixture.image, sessions.clone(), 1)
                .unwrap(),
        ),
        router: None,
        primary_request_projection: None,
        request_projections: HashMap::new(),
    };
    let invoke = |function, value| {
        runtime.execute_immediate_native("app.Sessions", function, vec![value], &mut |_| {})
    };
    assert_eq!(
        invoke("update", session.clone()).unwrap(),
        ReplValue::Bool(true)
    );
    let rotated = invoke("rotate", session.clone()).unwrap();
    assert_ne!(rotated, session);
    assert_eq!(
        invoke("read", rotated.clone()).unwrap(),
        ReplValue::String("retained".into())
    );
    assert!(
        invoke("read", session).is_err(),
        "old identity must be rejected"
    );
    assert_eq!(
        invoke("expire", rotated.clone()).unwrap(),
        ReplValue::Bool(true)
    );
    assert!(
        invoke("read", rotated).is_err(),
        "expired identity must be rejected"
    );
    assert!(sessions
        .with_runtime(|state| state.snapshots().is_empty())
        .unwrap());
    drop(runtime);
    std::fs::remove_dir_all(fixture.root).unwrap();
}
