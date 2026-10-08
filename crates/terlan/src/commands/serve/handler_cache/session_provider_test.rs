//! Canonical session declarations execute through their source provider.

use super::super::{AotHandlerGeneration, AotHandlerRuntime};
use super::compile_native_handler_fixture;
use crate::commands::serve::handler_cache::invocation::AotHandlerInvocationStep;
use crate::runtime::vm::ReplValue;
use std::collections::HashMap;
use std::sync::Arc;
use terlan_http_native::session_bindings::SessionStorage;

#[path = "session_acquisition_test.rs"]
mod acquisition;

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
    let sessions = super::super::session_service::new_session_service();
    let identity = sessions
        .with_storage(|state| state.create("", 86400))
        .unwrap()
        .unwrap();
    let session = ReplValue::Record {
        name: "Session".into(),
        fields: vec![
            ("identity".into(), ReplValue::String(identity.clone())),
            ("pending_identity".into(), ReplValue::String(identity)),
            ("ttl_seconds".into(), ReplValue::Int(86400)),
        ],
    };
    let load = |services| AotHandlerRuntime {
        module: "app.Sessions".into(),
        generation: Arc::new(
            AotHandlerGeneration::load_with_shard_count(&fixture.image, services, 1).unwrap(),
        ),
        router: None,
        primary_request_projection: None,
        request_projections: HashMap::new(),
    };
    let runtime = load(sessions.clone());
    let invoke = |runtime: &AotHandlerRuntime, function, value| {
        runtime
            .begin_request_invocation("app.Sessions", function, vec![value])
            .map(|step| {
                let AotHandlerInvocationStep::Complete(value) = step else {
                    panic!("registered session services must complete on the owner");
                };
                value
            })
    };
    let assert_stale = |runtime: &AotHandlerRuntime, function, value| {
        let error = invoke(runtime, function, value).expect_err("identity must be rejected");
        assert!(error.contains("stale HTTP session"), "{function}: {error}");
    };
    assert_eq!(
        invoke(&runtime, "update", session.clone()).unwrap(),
        ReplValue::Bool(true)
    );
    let replacement = load(sessions.clone());
    let isolated = load(super::super::session_service::new_session_service());
    for function in ["read", "expire"] {
        assert_stale(&isolated, function, session.clone());
    }
    assert_eq!(
        invoke(&replacement, "read", session.clone()).unwrap(),
        ReplValue::String("retained".into()),
        "a replacement code image shares the application's state"
    );
    let rotated = invoke(&replacement, "rotate", session.clone()).unwrap();
    assert_ne!(rotated, session);
    assert_eq!(
        invoke(&runtime, "read", rotated.clone()).unwrap(),
        ReplValue::String("retained".into())
    );
    for generation in [&runtime, &replacement] {
        assert_stale(generation, "read", session.clone());
    }
    drop(runtime);
    assert_eq!(
        invoke(&replacement, "read", rotated.clone()).unwrap(),
        ReplValue::String("retained".into()),
        "unloading the old image must not release application-owned resources"
    );
    assert_eq!(
        invoke(&replacement, "expire", rotated.clone()).unwrap(),
        ReplValue::Bool(true)
    );
    assert_stale(&replacement, "read", rotated);
    assert!(sessions.with_storage(|state| state.is_empty()).unwrap());
    drop(replacement);
    drop(isolated);
    std::fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn compiled_session_capabilities_reject_incompatible_context_results() {
    use crate::runtime::vm::package_native_helper::{execute_call, VmPackageNativeHelpers};
    use crate::runtime::vm::pure_native::PureNativeExecutionImage;
    use std::sync::Mutex;
    use terlan_runtime_abi::{NativeContextBinding, NativeServices, NativeValue};

    let fixture = compile_native_handler_fixture(
        "session_result_boundary",
        "src/app/SessionBoundary.terl",
        "app_SessionBoundary",
        r#"module app.SessionBoundary.
import std.core.Option.
@compiler.native {std.http.session.lookup}
pub lookup(identity: String): Option[String] -> native.
@compiler.native {std.http.session.create}
pub create(identity: String, ttl_seconds: Int): String -> native.
@compiler.native {std.http.session.get}
pub get(identity: String, key: String): Option[String] -> native.
@compiler.native {std.http.session.set}
pub set(identity: String, key: String, value: String): Unit -> native.
@compiler.native {std.http.session.delete}
pub delete(identity: String, key: String): Unit -> native.
@compiler.native {std.http.session.rotate}
pub rotate(identity: String, ttl_seconds: Int): String -> native.
@compiler.native {std.http.session.expire}
pub expire(identity: String): Unit -> native.
@compiler.native {std.http.session.is_live}
pub is_live(identity: String): Bool -> native.
pub healthy(): Int -> 42.
"#,
    );
    let count = Arc::new(Mutex::new(0));
    let mut services = NativeServices::default();
    let operations = [
        ("std.http.session.lookup", "lookup", 1),
        ("std.http.session.create", "create", 2),
        ("std.http.session.get", "get", 2),
        ("std.http.session.set", "set", 3),
        ("std.http.session.delete", "delete", 2),
        ("std.http.session.rotate", "rotate", 2),
        ("std.http.session.expire", "expire", 1),
        ("std.http.session.is_live", "is_live", 1),
    ];
    for (name, _, arity) in operations {
        services
            .register_context(
                Arc::clone(&count),
                [NativeContextBinding::new(name, arity, |count, _| {
                    *count += 1;
                    Ok(NativeValue::Int(99))
                })],
            )
            .unwrap();
    }
    let mut shard = PureNativeExecutionImage::load_with_native_services(&fixture.image, services)
        .unwrap()
        .spawn_shard()
        .unwrap();
    let mut helpers = VmPackageNativeHelpers::default();
    for (index, (_, export, arity)) in operations.iter().enumerate() {
        let mut args = vec![ReplValue::String("identity".into()); *arity];
        if matches!(*export, "create" | "rotate") {
            args[1] = ReplValue::Int(86400);
        }
        let error = execute_call(&mut shard, &mut helpers, export, &args).unwrap_err();
        let expected = if matches!(*export, "get" | "lookup") {
            "error[execution_shard.managed_layout]"
        } else {
            "error[execution_shard.type]"
        };
        assert!(error.contains(expected), "{export}: {error}");
        assert_eq!(
            *count.lock().unwrap(),
            index + 1,
            "no retry after an invalid result"
        );
        assert_eq!(
            execute_call(&mut shard, &mut helpers, "healthy", &[]).unwrap(),
            ReplValue::Int(42)
        );
    }
    drop(shard);
    std::fs::remove_dir_all(fixture.root).unwrap();
}
