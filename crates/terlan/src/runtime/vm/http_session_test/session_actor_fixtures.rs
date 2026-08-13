use super::super::live_template_command::VmHttpSessionCommandPayload;
use super::super::{
    created_session_table_id, current, delete, deleted_session_value, expire, get,
    http_message_id_to_int, resolve_http_session_affinity_key, rotate, set, with_response,
    VmHttpSession, VmHttpSessionAffinityError, VmHttpSessionAffinityKey,
    VmHttpSessionCommandOutcome, VmHttpSessionLiveTemplateSourceSpan,
    VmHttpSessionLiveTemplateSubscriber, VmHttpSessionLiveTemplateSubscriptionAuthorization,
    VmHttpSessionRuntime,
};
use crate::runtime::vm::process::{VmExitReason, VmProcessId};
use crate::runtime::vm::table::{VmTableEvent, VmTableId};
use crate::runtime::vm::ReplValue;

pub(super) fn apply_test_live_template_command(
    runtime: &mut VmHttpSessionRuntime,
    session: &VmHttpSession,
    payload: VmHttpSessionCommandPayload,
) -> Result<ReplValue, String> {
    runtime.write(session, "command", ReplValue::String(payload.name))?;
    Ok(ReplValue::String("applied".to_string()))
}

pub(super) fn write_test_count(
    runtime: &mut VmHttpSessionRuntime,
    session: &VmHttpSession,
) -> Result<(), String> {
    runtime.write(session, "count", ReplValue::Int(1))
}

pub(super) fn live_template_source() -> VmHttpSessionLiveTemplateSourceSpan {
    VmHttpSessionLiveTemplateSourceSpan::new("app.Dashboard", 12, 5)
        .expect("live-template source span")
}

#[test]
pub(super) fn http_session_lookup_creates_actor_and_sticky_metadata() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");

    let lookup = sessions
        .lookup_or_create(None)
        .expect("missing cookie should create session");

    assert_eq!(lookup.session.id, "s1");
    assert_eq!(lookup.route.node_id, "node-a");
    assert_eq!(lookup.route.session_id, "s1");
    assert_eq!(lookup.route.actor_pid, 1);
    assert_eq!(lookup.route.sticky_key, "node-a:s1");
    assert_eq!(
        lookup.set_cookie_header,
        Some("terlan_session=s1; Path=/; HttpOnly; SameSite=Lax".to_string())
    );

    let snapshots = sessions.snapshots();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].session_id, "s1");
    assert_eq!(snapshots[0].actor_pid, 1);
    assert_eq!(snapshots[0].table_id, 1);
    assert_eq!(snapshots[0].table_len, 0);
    assert_eq!(snapshots[0].live_template_subscriber_count, 0);
    assert_eq!(snapshots[0].actor_mailbox_len, 0);
    assert_eq!(snapshots[0].state_version, 0);
    assert_eq!(snapshots[0].expires_at_tick, 10);
    assert_eq!(snapshots[0].sticky_key, "node-a:s1");
}

#[test]
pub(super) fn http_session_adapter_functions_delegate_to_actor_runtime() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = current(&mut sessions, None).expect("current should create session");

    set(&mut sessions, &created.session, "user_id", "ada").expect("set should write");
    assert_eq!(
        get(&mut sessions, &created.session, "user_id").expect("get should read"),
        Some("ada".to_string())
    );

    let rotated = rotate(&mut sessions, &created.session).expect("rotate should succeed");
    assert_eq!(rotated.session.id, "s2");
    delete(&mut sessions, &rotated.session, "user_id").expect("delete should work");
    assert_eq!(
        get(&mut sessions, &rotated.session, "user_id").expect("deleted get should read"),
        None
    );
    let response = with_response("response", &rotated.session);
    assert_eq!(response, "response");

    expire(&mut sessions, &rotated.session).expect("expire should succeed");
    let error =
        get(&mut sessions, &rotated.session, "user_id").expect_err("expired get should fail");
    assert_eq!(error.domain(), terlan_runtime_abi::ErrorDomain::VmRuntime);
    assert_eq!(error.context(), "stale HTTP session `s2`");
}

#[test]
pub(super) fn http_session_adapter_renders_non_string_values_for_string_get() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = current(&mut sessions, None).expect("current should create session");

    sessions
        .write(&created.session, "count", ReplValue::Int(42))
        .expect("write should accept VM value");

    assert_eq!(
        get(&mut sessions, &created.session, "count").expect("get should render VM value"),
        Some("42".to_string())
    );
}

#[test]
pub(super) fn http_session_blank_cookie_creates_replacement_session() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");

    let lookup = current(&mut sessions, Some("   ")).expect("blank cookie should create session");

    assert_eq!(lookup.session.id, "s1");
    assert_eq!(
        lookup.set_cookie_header,
        Some("terlan_session=s1; Path=/; HttpOnly; SameSite=Lax".to_string())
    );
}

#[test]
pub(super) fn http_session_affinity_accepts_single_typed_key() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let affinity = [VmHttpSessionAffinityKey::new("route", "user:ada")];

    let lookup = sessions
        .lookup_or_create_with_affinity_keys(None, &affinity)
        .expect("single affinity key should allow session lookup");

    assert_eq!(lookup.session.id, "s1");
    assert_eq!(lookup.route.sticky_key, "node-a:s1");
    assert_eq!(
        resolve_http_session_affinity_key(&affinity).expect("affinity should resolve"),
        &affinity[0]
    );
}

#[test]
pub(super) fn http_session_affinity_merges_duplicate_matching_keys() {
    let affinity = [
        VmHttpSessionAffinityKey::new("route", "user:ada"),
        VmHttpSessionAffinityKey::new("middleware", "user:ada"),
    ];

    assert_eq!(
        resolve_http_session_affinity_key(&affinity).expect("matching keys should merge"),
        &affinity[0]
    );
}

#[test]
pub(super) fn http_session_affinity_rejects_missing_and_conflicting_keys() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let blank = [VmHttpSessionAffinityKey::new("route", "   ")];
    let conflict = [
        VmHttpSessionAffinityKey::new("route", "user:ada"),
        VmHttpSessionAffinityKey::new("middleware", "user:grace"),
    ];

    assert_eq!(
        resolve_http_session_affinity_key(&[]),
        Err(VmHttpSessionAffinityError::MissingAffinityKey)
    );
    assert_eq!(
        sessions
            .lookup_or_create_with_affinity_keys(None, &blank)
            .expect_err("blank affinity key should fail"),
        "missing HTTP session affinity key"
    );
    assert_eq!(
        sessions
            .lookup_or_create_with_affinity_keys(None, &conflict)
            .expect_err("conflicting keys should fail"),
        "conflicting HTTP session affinity keys: route requested `user:ada`, middleware requested `user:grace`"
    );
    assert_eq!(sessions.snapshots(), Vec::new());
}

#[test]
pub(super) fn http_session_table_event_adapters_are_defensive() {
    let table = VmTableId::from_raw_for_test(7);
    let owner = VmProcessId::from_raw_for_test(3);
    let key = ReplValue::String("k".to_string());
    let value = ReplValue::String("v".to_string());

    assert_eq!(
        created_session_table_id(VmTableEvent::Created { id: table, owner }),
        table
    );
    let panic = std::panic::catch_unwind(|| {
        created_session_table_id(VmTableEvent::Inserted {
            id: table,
            key: key.clone(),
        })
    })
    .expect_err("non-created event should panic");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .expect("panic should carry message");
    assert!(message.contains("unexpected HTTP session table creation event"));
    assert_eq!(deleted_session_value(None), None);
    assert_eq!(
        deleted_session_value(Some(VmTableEvent::Inserted {
            id: table,
            key: key.clone()
        })),
        None
    );
    assert_eq!(
        deleted_session_value(Some(VmTableEvent::Deleted {
            id: table,
            key,
            old_value: value.clone()
        })),
        Some(value)
    );
}

#[test]
pub(super) fn http_session_delete_reports_stale_table_after_internal_cleanup() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");
    sessions
        .write(
            &created.session,
            "user_id",
            ReplValue::String("ada".to_string()),
        )
        .expect("write should succeed");
    let actor = sessions.sessions.get("s1").expect("session record").actor;
    sessions.tables.cleanup_owner(actor);

    let error = sessions
        .delete(&created.session, "user_id")
        .expect_err("deleted table should fail");

    assert_eq!(error, "stale VM table handle 1");

    assert_eq!(
        sessions
            .bind_live_template_to_actor_state(&created.session, "dashboard", "user_id")
            .expect_err("binding through a stale table should fail"),
        "stale VM table handle 1"
    );
}

#[test]
pub(super) fn http_session_private_lookup_paths_report_stale_sessions() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");

    assert_eq!(
        sessions
            .lookup_existing("missing")
            .expect_err("missing lookup should fail"),
        "stale HTTP session `missing`"
    );
    assert_eq!(
        sessions
            .live_record("missing")
            .expect_err("missing live record should fail"),
        "stale HTTP session `missing`"
    );

    let created = sessions.lookup_or_create(None).expect("create session");
    sessions.advance_ticks(10);
    assert_eq!(
        sessions
            .live_record(&created.session.id)
            .expect_err("expired live record should clean itself up"),
        "stale HTTP session `s1`"
    );
    assert!(sessions.sessions.is_empty());
}

#[test]
pub(super) fn http_session_reuses_actor_and_table_state_for_cookie_lookup() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");

    sessions
        .write(
            &created.session,
            "user_id",
            ReplValue::String("ada".to_string()),
        )
        .expect("write should succeed");
    let reused = sessions
        .lookup_or_create(Some("s1"))
        .expect("cookie should reuse session");

    assert_eq!(reused.session, created.session);
    assert_eq!(reused.route.actor_pid, created.route.actor_pid);
    assert_eq!(reused.set_cookie_header, None);
    assert_eq!(
        sessions
            .read(&reused.session, "user_id")
            .expect("read should succeed"),
        Some(ReplValue::String("ada".to_string()))
    );
    assert_eq!(sessions.snapshots()[0].table_len, 1);

    assert_eq!(
        sessions
            .delete(&reused.session, "user_id")
            .expect("delete should succeed"),
        Some(ReplValue::String("ada".to_string()))
    );
    assert_eq!(
        sessions
            .read(&reused.session, "user_id")
            .expect("read after delete should succeed"),
        None
    );
}

#[test]
pub(super) fn http_session_actor_crash_during_request_cleans_state_and_replaces_cookie() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");
    sessions
        .write(
            &created.session,
            "cart",
            ReplValue::String("book".to_string()),
        )
        .expect("write should succeed");
    let actor = sessions.sessions.get("s1").expect("session record").actor;

    sessions
        .actors
        .exit_actor(actor, VmExitReason::Error("handler panic".to_string()))
        .expect("crash should be recorded");

    assert_eq!(
        sessions
            .read(&created.session, "cart")
            .expect_err("crashed session actor should fail current request"),
        "HTTP session actor `s1` crashed during request: process 1 exited with error `handler panic`"
    );
    assert_eq!(sessions.snapshots(), Vec::new());

    let replacement = sessions
        .lookup_or_create(Some("s1"))
        .expect("stale crashed cookie should create replacement");
    assert_eq!(replacement.session.id, "s2");
    assert_eq!(replacement.route.actor_pid, 2);
    assert_eq!(
        replacement.set_cookie_header,
        Some("terlan_session=s2; Path=/; HttpOnly; SameSite=Lax".to_string())
    );
    assert_eq!(
        sessions
            .read(&replacement.session, "cart")
            .expect("replacement session should be readable"),
        None
    );
}

#[test]
pub(super) fn http_session_reconnect_after_actor_crash_replaces_cookie_without_reusing_state() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");
    sessions
        .write(
            &created.session,
            "draft",
            ReplValue::String("old state".to_string()),
        )
        .expect("write should succeed");
    let actor = sessions.sessions.get("s1").expect("session record").actor;

    sessions
        .actors
        .exit_actor(actor, VmExitReason::Killed)
        .expect("crash should be recorded");
    let replacement = sessions
        .lookup_or_create(Some("s1"))
        .expect("reconnect after crash should create replacement");

    assert_eq!(replacement.session.id, "s2");
    assert_eq!(replacement.route.actor_pid, 2);
    assert_eq!(
        replacement.set_cookie_header,
        Some("terlan_session=s2; Path=/; HttpOnly; SameSite=Lax".to_string())
    );
    assert_eq!(
        sessions
            .read(&replacement.session, "draft")
            .expect("replacement read should succeed"),
        None
    );
    let snapshots = sessions.snapshots();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].session_id, "s2");
    assert_eq!(snapshots[0].table_len, 0);
}

#[test]
pub(super) fn http_session_idempotent_command_replays_duplicate_result_without_rerun() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");
    let mut calls = 0;

    let first = sessions
        .apply_idempotent_command(&created.session, " command-1 ", |runtime, session| {
            calls += 1;
            runtime.write(session, "counter", ReplValue::Int(calls))?;
            Ok(ReplValue::String("created".to_string()))
        })
        .expect("first command should apply");
    let second = sessions
        .apply_idempotent_command(&created.session, "command-1", |_runtime, _session| {
            panic!("duplicate command id must replay without rerunning handler")
        })
        .expect("duplicate command should replay");

    assert_eq!(
        first,
        VmHttpSessionCommandOutcome::Applied(ReplValue::String("created".to_string()))
    );
    assert_eq!(
        second,
        VmHttpSessionCommandOutcome::Replayed(ReplValue::String("created".to_string()))
    );
    assert_eq!(calls, 1);
    assert_eq!(
        sessions
            .read(&created.session, "counter")
            .expect("counter should remain from first command"),
        Some(ReplValue::Int(1))
    );
    assert_eq!(
        sessions
            .apply_idempotent_command(&created.session, "   ", |_runtime, _session| {
                Ok(ReplValue::Unit)
            })
            .expect_err("blank command id should fail"),
        "HTTP session command id cannot be empty"
    );
}

#[test]
pub(super) fn http_session_rejects_malformed_live_template_command_payload_before_dispatch() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");
    let command: fn(
        &mut VmHttpSessionRuntime,
        &VmHttpSession,
        VmHttpSessionCommandPayload,
    ) -> Result<ReplValue, String> = apply_test_live_template_command;

    assert_eq!(
        VmHttpSessionCommandPayload::new(
            " command-0 ",
            " refresh ",
            ReplValue::String("seed".to_string())
        )
        .expect("payload should normalize"),
        VmHttpSessionCommandPayload {
            command_id: "command-0".to_string(),
            name: "refresh".to_string(),
            body: ReplValue::String("seed".to_string()),
        }
    );
    assert_eq!(
        sessions
            .apply_live_template_command(
                &created.session,
                "   ",
                "save",
                ReplValue::String("payload".to_string()),
                command,
            )
            .expect_err("blank command id should fail"),
        "HTTP session command id cannot be empty"
    );
    assert_eq!(
        sessions
            .apply_live_template_command(
                &created.session,
                "command-2",
                "   ",
                ReplValue::String("payload".to_string()),
                command,
            )
            .expect_err("blank command name should fail"),
        "HTTP live-template command name cannot be empty"
    );
    assert_eq!(
        sessions
            .apply_live_template_command(
                &created.session,
                "command-3",
                "save\nnow",
                ReplValue::String("payload".to_string()),
                command,
            )
            .expect_err("control-character command name should fail"),
        "HTTP live-template command name cannot contain control characters"
    );

    let first = sessions
        .apply_live_template_command(
            &created.session,
            " command-4 ",
            " save ",
            ReplValue::String("payload".to_string()),
            command,
        )
        .expect("valid command should apply");
    let replayed = sessions
        .apply_live_template_command(
            &created.session,
            "command-4",
            "save",
            ReplValue::String("different payload".to_string()),
            command,
        )
        .expect("duplicate command should replay");

    assert_eq!(
        first,
        VmHttpSessionCommandOutcome::Applied(ReplValue::String("applied".to_string()))
    );
    assert_eq!(
        replayed,
        VmHttpSessionCommandOutcome::Replayed(ReplValue::String("applied".to_string()))
    );
    assert_eq!(
        sessions
            .read(&created.session, "command")
            .expect("command value should read"),
        Some(ReplValue::String("save".to_string()))
    );
}

#[test]
pub(super) fn http_session_live_template_command_dispatches_actor_mailbox_postback_once() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");

    let first = sessions
        .dispatch_live_template_command_to_actor_mailbox(
            &created.session,
            " command-5 ",
            " increment ",
            ReplValue::String("1".to_string()),
        )
        .expect("first command should dispatch");
    let replayed = sessions
        .dispatch_live_template_command_to_actor_mailbox(
            &created.session,
            "command-5",
            "increment",
            ReplValue::String("2".to_string()),
        )
        .expect("duplicate command should replay");

    let dispatched = ReplValue::Tuple(vec![
        ReplValue::Atom("live_template_command_dispatched".to_string()),
        ReplValue::Int(1),
    ]);
    assert_eq!(
        first,
        VmHttpSessionCommandOutcome::Applied(dispatched.clone())
    );
    assert_eq!(replayed, VmHttpSessionCommandOutcome::Replayed(dispatched));
    assert_eq!(sessions.snapshots()[0].actor_mailbox_len, 1);

    assert_eq!(
        sessions
            .receive_actor_message(&created.session)
            .expect("mailbox receive should succeed")
            .expect("mailbox should contain command postback"),
        ReplValue::Tuple(vec![
            ReplValue::Atom("live_template_command".to_string()),
            ReplValue::String("command-5".to_string()),
            ReplValue::String("increment".to_string()),
            ReplValue::String("1".to_string()),
        ])
    );
    assert_eq!(sessions.snapshots()[0].actor_mailbox_len, 0);
    assert_eq!(
        sessions
            .receive_actor_message(&created.session)
            .expect("empty mailbox receive should succeed"),
        None
    );
}

#[test]
pub(super) fn http_session_message_id_conversion_rejects_values_beyond_int() {
    assert_eq!(http_message_id_to_int(i64::MAX as u64), Ok(i64::MAX));
    assert_eq!(
        http_message_id_to_int(i64::MAX as u64 + 1).expect_err("message id beyond Int should fail"),
        "HTTP live-template command message id overflowed Int"
    );
}

#[test]
pub(super) fn http_session_live_template_subscribers_are_cleaned_after_actor_exit() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");

    assert_eq!(
        sessions
            .subscribe_live_template(&created.session, " stream-1 ", " sse ")
            .expect("subscriber should register"),
        VmHttpSessionLiveTemplateSubscriber {
            id: "stream-1".to_string(),
            transport: "sse".to_string(),
        }
    );
    assert_eq!(
        sessions
            .live_template_subscribers(&created.session)
            .expect("subscribers should list"),
        vec![VmHttpSessionLiveTemplateSubscriber {
            id: "stream-1".to_string(),
            transport: "sse".to_string(),
        }]
    );
    assert_eq!(sessions.snapshots()[0].live_template_subscriber_count, 1);
    assert_eq!(
        sessions
            .unsubscribe_live_template(&created.session, " stream-1 ")
            .expect("subscriber should unregister"),
        Some(VmHttpSessionLiveTemplateSubscriber {
            id: "stream-1".to_string(),
            transport: "sse".to_string(),
        })
    );
    assert_eq!(
        sessions
            .unsubscribe_live_template(&created.session, "stream-1")
            .expect("duplicate unsubscribe should be idempotent"),
        None
    );
    assert_eq!(
        sessions
            .unsubscribe_live_template(&created.session, "   ")
            .expect_err("blank unsubscribe id should fail"),
        "HTTP live-template subscriber id cannot be empty"
    );
    sessions
        .subscribe_live_template(&created.session, "stream-1", "sse")
        .expect("subscriber should register again");
    assert_eq!(
        sessions
            .subscribe_live_template(&created.session, "   ", "sse")
            .expect_err("blank subscriber id should fail"),
        "HTTP live-template subscriber id cannot be empty"
    );
    assert_eq!(
        sessions
            .subscribe_live_template(&created.session, "stream-2", "   ")
            .expect_err("blank subscriber transport should fail"),
        "HTTP live-template subscriber transport cannot be empty"
    );

    let actor = sessions.sessions.get("s1").expect("session record").actor;
    sessions
        .actors
        .exit_actor(actor, VmExitReason::Killed)
        .expect("actor exit should be recorded");

    assert_eq!(
        sessions
            .live_template_subscribers(&created.session)
            .expect_err("crashed session should clean subscriber state"),
        "HTTP session actor `s1` crashed during request: process 1 exited with killed"
    );
    assert_eq!(sessions.snapshots(), Vec::new());

    let replacement = sessions
        .lookup_or_create(Some("s1"))
        .expect("stale cookie should create replacement");
    assert_eq!(replacement.session.id, "s2");
    assert_eq!(
        sessions
            .live_template_subscribers(&replacement.session)
            .expect("replacement should have no stale subscribers"),
        Vec::<VmHttpSessionLiveTemplateSubscriber>::new()
    );
    assert_eq!(sessions.snapshots()[0].live_template_subscriber_count, 0);
}

#[test]
pub(super) fn http_session_live_template_subscription_requires_capability_before_registering() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");

    assert_eq!(
        sessions
            .subscribe_live_template_with_capability(
                &created.session,
                "admin-panel",
                "sse",
                "template:admin",
                &["template:public"],
            )
            .expect_err("missing capability should reject before registration"),
        "HTTP live-template subscriber `admin-panel` missing capability `template:admin`"
    );
    assert_eq!(sessions.snapshots()[0].live_template_subscriber_count, 0);

    for (subscriber, transport, capability, expected) in [
        (
            "   ",
            "sse",
            "template:admin",
            "HTTP live-template subscriber id cannot be empty",
        ),
        (
            "admin-panel",
            "   ",
            "template:admin",
            "HTTP live-template subscriber transport cannot be empty",
        ),
        (
            "admin-panel",
            "sse",
            "   ",
            "HTTP live-template subscriber capability cannot be empty",
        ),
    ] {
        assert_eq!(
            sessions
                .subscribe_live_template_with_capability(
                    &created.session,
                    subscriber,
                    transport,
                    capability,
                    &["template:admin"],
                )
                .expect_err("blank capability input should fail"),
            expected
        );
    }

    assert_eq!(
        sessions
            .subscribe_live_template_with_capability(
                &created.session,
                " admin-panel ",
                " sse ",
                " template:admin ",
                &["template:public", " template:admin ", "template:admin"],
            )
            .expect("capability should authorize subscription"),
        VmHttpSessionLiveTemplateSubscriptionAuthorization {
            subscriber: VmHttpSessionLiveTemplateSubscriber {
                id: "admin-panel".to_string(),
                transport: "sse".to_string(),
            },
            required_capability: "template:admin".to_string(),
            granted_capabilities: vec!["template:admin".to_string(), "template:public".to_string()],
            diagnostic:
                "HTTP live-template subscriber `admin-panel` authorized with capability `template:admin`"
                    .to_string(),
        }
    );
    assert_eq!(
        sessions
            .live_template_subscribers(&created.session)
            .expect("authorized subscriber should list"),
        vec![VmHttpSessionLiveTemplateSubscriber {
            id: "admin-panel".to_string(),
            transport: "sse".to_string(),
        }]
    );
    assert_eq!(
        sessions
            .subscribe_live_template_with_capability(
                &created.session,
                "audit-panel",
                "sse",
                "template:audit",
                &["   "],
            )
            .expect_err("blank granted capability should fail"),
        "HTTP live-template granted capability cannot be empty"
    );
}
