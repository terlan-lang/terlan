
#[test]
fn http_session_binds_typed_live_template_to_actor_state() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");

    sessions
        .subscribe_live_template(&created.session, "summary", "sse")
        .expect("summary subscriber should register");
    sessions
        .subscribe_live_template(&created.session, "details", "websocket")
        .expect("details subscriber should register");
    let observed_version = sessions
        .state_version(&created.session)
        .expect("version should read");
    sessions
        .apply_state_update(&created.session, observed_version, |runtime, session| {
            runtime.write(session, "count", ReplValue::Int(7))
        })
        .expect("state should update");

    assert_eq!(
        sessions
            .bind_live_template_to_actor_state(&created.session, "   ", "count")
            .expect_err("blank template id should fail"),
        "HTTP live-template id cannot be empty"
    );
    assert_eq!(
        sessions
            .bind_live_template_to_actor_state(&created.session, "dashboard.counter", "   ")
            .expect_err("blank state key should fail"),
        "HTTP live-template state key cannot be empty"
    );

    assert_eq!(
        sessions
            .bind_live_template_to_actor_state(&created.session, " dashboard.counter ", " count ")
            .expect("template should bind to actor state"),
        VmHttpSessionLiveTemplateActorBinding {
            session_id: "s1".to_string(),
            actor_pid: 1,
            table_id: 1,
            template_id: "dashboard.counter".to_string(),
            state_key: "count".to_string(),
            state_value: Some(ReplValue::Int(7)),
            state_version: 1,
            live_template_subscriber_count: 2,
            diagnostic:
                "HTTP live-template `dashboard.counter` bound to session `s1` actor 1 state `count`"
                    .to_string(),
        }
    );
    assert_eq!(
        sessions
            .bind_live_template_to_actor_state(&created.session, "dashboard.missing", "missing")
            .expect("missing state should still bind with typed none"),
        VmHttpSessionLiveTemplateActorBinding {
            session_id: "s1".to_string(),
            actor_pid: 1,
            table_id: 1,
            template_id: "dashboard.missing".to_string(),
            state_key: "missing".to_string(),
            state_value: None,
            state_version: 1,
            live_template_subscriber_count: 2,
            diagnostic:
                "HTTP live-template `dashboard.missing` bound to session `s1` actor 1 state `missing`"
                    .to_string(),
        }
    );
    assert_eq!(
        sessions
            .bind_live_template_to_actor_state(&created.session, "   ", "count")
            .expect_err("blank template should fail closed"),
        "HTTP live-template id cannot be empty"
    );
    assert_eq!(
        sessions
            .bind_live_template_to_actor_state(&created.session, "dashboard.counter", "   ")
            .expect_err("blank state key should fail closed"),
        "HTTP live-template state key cannot be empty"
    );

    let actor = sessions.sessions.get("s1").expect("session record").actor;
    sessions
        .actors
        .exit_actor(actor, VmExitReason::Killed)
        .expect("actor exit should be recorded");
    assert_eq!(
        sessions
            .bind_live_template_to_actor_state(&created.session, "dashboard.counter", "count")
            .expect_err("crashed actor must reject binding"),
        "HTTP session actor `s1` crashed during request: process 1 exited with killed"
    );
}

#[test]
fn http_session_traces_live_template_subscription_source_map() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");

    sessions
        .subscribe_live_template(&created.session, "summary", "sse")
        .expect("subscriber should register");
    let observed_version = sessions
        .state_version(&created.session)
        .expect("version should read");
    sessions
        .apply_state_update(&created.session, observed_version, |runtime, session| {
            runtime.write(session, "count", ReplValue::Int(3))
        })
        .expect("state should update");

    assert_eq!(
        sessions
            .trace_live_template_subscription_with_source_map(
                &created.session,
                " summary ",
                " dashboard.counter ",
                " app.Dashboard ",
                12,
                5,
            )
            .expect("subscription trace should be source-map aware"),
        VmHttpSessionLiveTemplateSubscriptionTrace {
            session_id: "s1".to_string(),
            actor_pid: 1,
            subscriber_id: "summary".to_string(),
            transport: "sse".to_string(),
            template_id: "dashboard.counter".to_string(),
            source_module: "app.Dashboard".to_string(),
            source_line: 12,
            source_column: 5,
            state_version: 1,
            diagnostic:
                "HTTP live-template `dashboard.counter` subscriber `summary` on session `s1` traced to app.Dashboard:12:5"
                    .to_string(),
        }
    );
    assert_eq!(
        sessions
            .trace_live_template_subscription_with_source_map(
                &created.session,
                "missing",
                "dashboard.counter",
                "app.Dashboard",
                12,
                5,
            )
            .expect_err("missing subscriber should fail closed"),
        "HTTP live-template `dashboard.counter` cannot trace missing subscriber `missing`"
    );
    assert_eq!(
        sessions
            .trace_live_template_subscription_with_source_map(
                &created.session,
                "   ",
                "dashboard.counter",
                "app.Dashboard",
                12,
                5,
            )
            .expect_err("blank subscriber should fail closed"),
        "HTTP live-template subscriber id cannot be empty"
    );
    assert_eq!(
        sessions
            .trace_live_template_subscription_with_source_map(
                &created.session,
                "summary",
                "   ",
                "app.Dashboard",
                12,
                5,
            )
            .expect_err("blank template should fail closed"),
        "HTTP live-template id cannot be empty"
    );
    assert_eq!(
        sessions
            .trace_live_template_subscription_with_source_map(
                &created.session,
                "summary",
                "dashboard.counter",
                "   ",
                12,
                5,
            )
            .expect_err("blank module should fail closed"),
        "HTTP live-template source module cannot be empty"
    );
    assert_eq!(
        sessions
            .trace_live_template_subscription_with_source_map(
                &created.session,
                "summary",
                "dashboard.counter",
                "app.Dashboard",
                0,
                5,
            )
            .expect_err("zero source line should fail closed"),
        "HTTP live-template source line must be greater than 0"
    );
    assert_eq!(
        sessions
            .trace_live_template_subscription_with_source_map(
                &created.session,
                "summary",
                "dashboard.counter",
                "app.Dashboard",
                12,
                0,
            )
            .expect_err("zero source column should fail closed"),
        "HTTP live-template source column must be greater than 0"
    );

    let actor = sessions.sessions.get("s1").expect("session record").actor;
    sessions
        .actors
        .exit_actor(actor, VmExitReason::Killed)
        .expect("actor exit should be recorded");
    assert_eq!(
        sessions
            .trace_live_template_subscription_with_source_map(
                &created.session,
                "summary",
                "dashboard.counter",
                "app.Dashboard",
                12,
                5,
            )
            .expect_err("crashed actor must reject trace"),
        "HTTP session actor `s1` crashed during request: process 1 exited with killed"
    );
}

#[test]
fn http_session_live_template_state_update_fans_out_to_all_subscribers() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");

    sessions
        .subscribe_live_template(&created.session, "summary", "sse")
        .expect("summary subscriber should register");
    sessions
        .subscribe_live_template(&created.session, "details", "websocket")
        .expect("details subscriber should register");
    let observed_version = sessions
        .state_version(&created.session)
        .expect("version should read");
    let update: fn(&mut VmHttpSessionRuntime, &VmHttpSession) -> Result<(), String> =
        write_test_count;

    let fanout = sessions
        .fanout_live_template_state_update(
            &created.session,
            observed_version,
            " cart.patch ",
            &live_template_source(),
            ReplValue::String("{\"count\":1}".to_string()),
            update,
        )
        .expect("state update should fan out");

    assert_eq!(
        fanout,
        VmHttpSessionLiveTemplateStateFanout {
            session_id: "s1".to_string(),
            state_version: 1,
            patch_event: "cart.patch".to_string(),
            subscriber_events: vec![
                VmHttpSessionLiveTemplateFanoutEvent {
                    subscriber_id: "details".to_string(),
                    transport: "websocket".to_string(),
                    event_id: "s1:1:details".to_string(),
                    event_name: "cart.patch".to_string(),
                    payload: ReplValue::Tuple(vec![
                        ReplValue::Atom("live_template_state_update".to_string()),
                        ReplValue::String("cart.patch".to_string()),
                        ReplValue::Int(1),
                        ReplValue::String("{\"count\":1}".to_string()),
                    ]),
                },
                VmHttpSessionLiveTemplateFanoutEvent {
                    subscriber_id: "summary".to_string(),
                    transport: "sse".to_string(),
                    event_id: "s1:1:summary".to_string(),
                    event_name: "cart.patch".to_string(),
                    payload: ReplValue::Tuple(vec![
                        ReplValue::Atom("live_template_state_update".to_string()),
                        ReplValue::String("cart.patch".to_string()),
                        ReplValue::Int(1),
                        ReplValue::String("{\"count\":1}".to_string()),
                    ]),
                },
            ],
        }
    );
    assert_eq!(
        sessions
            .read(&created.session, "count")
            .expect("updated state should read"),
        Some(ReplValue::Int(1))
    );
    assert_eq!(
        sessions
            .fanout_live_template_state_update(
                &created.session,
                observed_version,
                "cart.patch",
                &live_template_source(),
                ReplValue::String("{\"count\":2}".to_string()),
                update,
            )
            .expect_err("stale writer should fail before fanout"),
        "HTTP session `s1` state version conflict: expected 0, actual 1"
    );
    assert_eq!(
        sessions
            .fanout_live_template_state_update(
                &created.session,
                1,
                "   ",
                &live_template_source(),
                ReplValue::String("{\"count\":2}".to_string()),
                update,
            )
            .expect_err("blank patch event should fail"),
        "HTTP live-template patch event cannot be empty"
    );
    assert_eq!(
        sessions
            .read(&created.session, "count")
            .expect("failed fanout should not mutate state"),
        Some(ReplValue::Int(1))
    );
}

#[test]
fn http_session_live_template_state_update_rejects_int_overflow() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");
    sessions
        .sessions
        .get_mut(&created.session.id)
        .expect("session record")
        .state_version = u64::MAX;
    let update: fn(&mut VmHttpSessionRuntime, &VmHttpSession) -> Result<(), String> =
        write_test_count;

    assert_eq!(
        sessions
            .fanout_live_template_state_update(
                &created.session,
                u64::MAX,
                "cart.patch",
                &live_template_source(),
                ReplValue::Unit,
                update,
            )
            .expect_err("state version beyond Int should fail"),
        "HTTP live-template state version overflowed Int"
    );
}

#[test]
fn http_session_state_update_rejects_stale_concurrent_writer() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");
    let observed_version = sessions
        .state_version(&created.session)
        .expect("state version should read");
    let update: fn(&mut VmHttpSessionRuntime, &VmHttpSession) -> Result<(), String> =
        write_test_count;

    let next_version = sessions
        .apply_state_update(&created.session, observed_version, update)
        .expect("first update should apply");

    assert_eq!(next_version, 1);
    assert_eq!(
        sessions
            .apply_state_update(&created.session, observed_version, update)
            .expect_err("stale concurrent writer should fail"),
        "HTTP session `s1` state version conflict: expected 0, actual 1"
    );
    assert_eq!(
        sessions
            .read(&created.session, "count")
            .expect("read should succeed"),
        Some(ReplValue::Int(1))
    );
    assert_eq!(
        sessions
            .state_version(&created.session)
            .expect("version should remain after rejected stale update"),
        1
    );

    let final_version = sessions
        .apply_state_update(&created.session, next_version, update)
        .expect("fresh version should apply");

    assert_eq!(final_version, 2);
    assert_eq!(sessions.snapshots()[0].state_version, 2);
    assert_eq!(
        sessions
            .read(&created.session, "count")
            .expect("read should succeed"),
        Some(ReplValue::Int(1))
    );
}

#[test]
fn http_session_persistence_snapshot_replays_after_restart() {
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
    sessions
        .subscribe_live_template(&created.session, "stream-1", "sse")
        .expect("subscriber should register");
    let version = sessions
        .state_version(&created.session)
        .expect("version should read");
    sessions
        .apply_state_update(&created.session, version, |runtime, session| {
            runtime.write(session, "count", ReplValue::Int(1))
        })
        .expect("state update should apply");
    sessions
        .apply_idempotent_command(&created.session, "command-1", |runtime, session| {
            runtime.write(
                session,
                "last_command",
                ReplValue::String("applied".to_string()),
            )?;
            Ok(ReplValue::String("ok".to_string()))
        })
        .expect("command should apply");

    let snapshot = sessions
        .persistence_snapshot(&created.session)
        .expect("snapshot should export durable state");
    let mut restarted =
        VmHttpSessionRuntime::new("node-b", 10).expect("restart runtime should create");
    let restored = restarted
        .replay_persistence_snapshot(snapshot.clone())
        .expect("snapshot should replay after restart");

    assert_eq!(restored.session.id, "s1");
    assert_eq!(restored.route.node_id, "node-b");
    assert_eq!(restored.route.actor_pid, 1);
    assert_eq!(
        restored.set_cookie_header,
        Some("terlan_session=s1; Path=/; HttpOnly; SameSite=Lax".to_string())
    );
    assert_eq!(
        restarted
            .read(&restored.session, "cart")
            .expect("restored cart should read"),
        Some(ReplValue::String("book".to_string()))
    );
    assert_eq!(
        restarted
            .read(&restored.session, "count")
            .expect("restored count should read"),
        Some(ReplValue::Int(1))
    );
    assert_eq!(
        restarted
            .live_template_subscribers(&restored.session)
            .expect("transient subscribers should not persist"),
        Vec::<VmHttpSessionLiveTemplateSubscriber>::new()
    );
    assert_eq!(restarted.snapshots()[0].state_version, 1);
    assert_eq!(restarted.snapshots()[0].table_len, 3);

    let replayed = restarted
        .apply_idempotent_command(&restored.session, "command-1", |_runtime, _session| {
            panic!("replayed command must not run after persistence restore")
        })
        .expect("restored command cache should replay");
    assert_eq!(
        replayed,
        VmHttpSessionCommandOutcome::Replayed(ReplValue::String("ok".to_string()))
    );
    assert_eq!(
        restarted
            .replay_persistence_snapshot(snapshot.clone())
            .expect_err("duplicate replay should fail closed"),
        "HTTP session persistence snapshot `s1` would overwrite live session"
    );

    let next = restarted
        .lookup_or_create(None)
        .expect("allocation should continue after replay");
    assert_eq!(next.session.id, "s2");

    let mut expired =
        VmHttpSessionRuntime::new("node-c", 10).expect("expired runtime should create");
    expired.advance_ticks(10);
    assert_eq!(
        expired
            .replay_persistence_snapshot(snapshot)
            .expect_err("expired snapshot should fail"),
        "HTTP session persistence snapshot `s1` is expired"
    );
}

#[test]
fn http_session_persistence_rejects_blank_ids_and_accepts_external_ids() {
    let mut source = VmHttpSessionRuntime::new("node-a", 10).expect("source runtime should create");
    let created = source.lookup_or_create(None).expect("create session");
    let snapshot = source
        .persistence_snapshot(&created.session)
        .expect("snapshot should export");

    let mut blank = snapshot.clone();
    blank.session_id = "   ".to_string();
    let mut target = VmHttpSessionRuntime::new("node-b", 10).expect("target runtime should create");
    assert_eq!(
        target
            .replay_persistence_snapshot(blank)
            .expect_err("blank persistence id should fail"),
        "HTTP session persistence snapshot id cannot be empty"
    );

    let mut external = snapshot;
    external.session_id = "external-session".to_string();
    let restored = target
        .replay_persistence_snapshot(external)
        .expect("non-numeric session id should replay");
    assert_eq!(restored.session.id, "external-session");
    assert_eq!(
        target
            .lookup_or_create(None)
            .expect("numeric allocation should remain available")
            .session
            .id,
        "s1"
    );
}

#[test]
fn http_session_reports_normal_and_missing_actor_exit_reasons() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");
    let actor = sessions.sessions.get("s1").expect("session record").actor;

    sessions
        .actors
        .exit_actor(actor, VmExitReason::Normal)
        .expect("session actor should exit");
    assert_eq!(
        sessions
            .read(&created.session, "value")
            .expect_err("normal actor exit should invalidate the session"),
        "HTTP session actor `s1` crashed during request: process 1 exited with normal exit"
    );

    assert_eq!(
        sessions.session_actor_exit_reason(VmProcessId::from_raw_for_test(999)),
        Some(VmExitReason::Error("missing actor process".to_string()))
    );
}

#[test]
fn http_session_actor_mailbox_backpressure_is_attributed() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");

    assert_eq!(
        sessions
            .actor_mailbox_backpressure(&created.session, 0)
            .expect_err("zero threshold should fail"),
        "HTTP session actor mailbox backpressure threshold must be greater than 0"
    );
    assert_eq!(
        sessions
            .actor_mailbox_backpressure(&created.session, 2)
            .expect("empty mailbox should inspect"),
        VmHttpSessionMailboxBackpressure {
            session_id: "s1".to_string(),
            actor_pid: 1,
            mailbox_len: 0,
            threshold: 2,
            saturated: false,
            attribution:
                "HTTP session `s1` actor mailbox pressure is within threshold: 0 queued messages < threshold 2"
                    .to_string(),
        }
    );

    sessions
        .enqueue_actor_message(&created.session, ReplValue::String("one".to_string()))
        .expect("first message should enqueue");
    sessions
        .enqueue_actor_message(&created.session, ReplValue::String("two".to_string()))
        .expect("second message should enqueue");

    assert_eq!(
        sessions
            .actor_mailbox_backpressure(&created.session, 2)
            .expect("saturated mailbox should inspect"),
        VmHttpSessionMailboxBackpressure {
            session_id: "s1".to_string(),
            actor_pid: 1,
            mailbox_len: 2,
            threshold: 2,
            saturated: true,
            attribution:
                "HTTP session `s1` actor mailbox backpressure: 2 queued messages >= threshold 2"
                    .to_string(),
        }
    );
    assert_eq!(sessions.snapshots()[0].actor_mailbox_len, 2);
}

#[test]
fn http_session_migrates_durable_state_across_workers() {
    let mut source = VmHttpSessionRuntime::new("node-a", 10).expect("source runtime should create");
    let created = source.lookup_or_create(None).expect("create session");
    source
        .write(
            &created.session,
            "cart",
            ReplValue::String("book".to_string()),
        )
        .expect("write should succeed");
    source
        .apply_idempotent_command(&created.session, "command-1", |runtime, session| {
            runtime.write(
                session,
                "last_command",
                ReplValue::String("applied".to_string()),
            )?;
            Ok(ReplValue::String("ok".to_string()))
        })
        .expect("command should apply");
    source
        .subscribe_live_template(&created.session, "stream-1", "sse")
        .expect("transient subscriber should register");

    let mut duplicate_destination =
        VmHttpSessionRuntime::new("node-c", 10).expect("duplicate destination should create");
    duplicate_destination
        .lookup_or_create(None)
        .expect("duplicate destination should already own s1");
    assert_eq!(
        source
            .migrate_to_worker(&created.session, &mut duplicate_destination)
            .expect_err("duplicate destination session should fail closed"),
        "HTTP session persistence snapshot `s1` would overwrite live session"
    );
    assert_eq!(
        source
            .read(&created.session, "cart")
            .expect("failed migration must leave source state intact"),
        Some(ReplValue::String("book".to_string()))
    );

    let mut destination =
        VmHttpSessionRuntime::new("node-b", 10).expect("destination runtime should create");
    let migration = source
        .migrate_to_worker(&created.session, &mut destination)
        .expect("migration should succeed");

    assert_eq!(
        migration,
        VmHttpSessionWorkerMigration {
            session_id: "s1".to_string(),
            source_route: created.route.clone(),
            destination_route: destination.snapshots().into_iter().next().map_or_else(
                || panic!("destination snapshot should exist"),
                |snapshot| super::VmHttpSessionRoute {
                    node_id: "node-b".to_string(),
                    session_id: snapshot.session_id,
                    actor_pid: snapshot.actor_pid,
                    sticky_key: snapshot.sticky_key,
                },
            ),
            set_cookie_header: Some(
                "terlan_session=s1; Path=/; HttpOnly; SameSite=Lax".to_string(),
            ),
            diagnostic:
                "HTTP session `s1` migrated from worker `node-a` to worker `node-b` as actor 1"
                    .to_string(),
        }
    );
    assert_eq!(source.snapshots(), Vec::new());
    assert_eq!(
        source
            .read(&created.session, "cart")
            .expect_err("source should no longer own migrated session"),
        "stale HTTP session `s1`"
    );
    assert_eq!(
        destination
            .read(&created.session, "cart")
            .expect("destination should own migrated state"),
        Some(ReplValue::String("book".to_string()))
    );
    assert_eq!(
        destination
            .read(&created.session, "last_command")
            .expect("destination should own command side effect"),
        Some(ReplValue::String("applied".to_string()))
    );
    assert_eq!(
        destination
            .live_template_subscribers(&created.session)
            .expect("transient subscribers should not migrate"),
        Vec::<VmHttpSessionLiveTemplateSubscriber>::new()
    );
    assert_eq!(
        destination
            .apply_idempotent_command(&created.session, "command-1", |_runtime, _session| {
                panic!("migrated command result should replay")
            })
            .expect("migrated command cache should replay"),
        VmHttpSessionCommandOutcome::Replayed(ReplValue::String("ok".to_string()))
    );

    let mut same_worker =
        VmHttpSessionRuntime::new("node-b", 10).expect("same worker runtime should create");
    assert_eq!(
        destination
            .migrate_to_worker(&created.session, &mut same_worker)
            .expect_err("same-worker migration should fail"),
        "HTTP session `s1` migration target must be a different worker"
    );
}

#[test]
fn http_session_reports_hot_reload_migration_compatibility() {
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
    sessions
        .apply_idempotent_command(&created.session, "command-1", |runtime, session| {
            runtime.write(
                session,
                "last_command",
                ReplValue::String("applied".to_string()),
            )?;
            Ok(ReplValue::String("ok".to_string()))
        })
        .expect("command should apply");
    sessions
        .subscribe_live_template(&created.session, "stream-1", "sse")
        .expect("subscriber should register");

    assert_eq!(
        sessions
            .hot_reload_migration_compatibility_report(&created.session, 1, 2)
            .expect("hot reload report should succeed"),
        VmHttpSessionHotReloadMigrationReport {
            session_id: "s1".to_string(),
            previous_generation: 1,
            active_generation: 2,
            compatible: true,
            durable_table_entries: 2,
            durable_command_results: 1,
            transient_subscribers: 1,
            diagnostic:
                "HTTP session `s1` is compatible with hot reload generation 1->2: 2 table entries and 1 command results remain durable; 1 live-template subscribers remain transient"
                    .to_string(),
        }
    );
    assert_eq!(
        sessions
            .hot_reload_migration_compatibility_report(&created.session, 2, 2)
            .expect_err("same generation should fail"),
        "HTTP session `s1` hot reload report requires distinct generations"
    );

    sessions
        .expire(&created.session)
        .expect("session should expire");
    assert_eq!(
        sessions
            .hot_reload_migration_compatibility_report(&created.session, 2, 3)
            .expect_err("expired session should fail"),
        "stale HTTP session `s1`"
    );
}

#[test]
fn http_session_rotate_changes_cookie_without_losing_actor_state() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");
    sessions
        .write(
            &created.session,
            "role",
            ReplValue::String("admin".to_string()),
        )
        .expect("write should succeed");

    let rotated = sessions
        .rotate(&created.session)
        .expect("rotation should succeed");

    assert_eq!(rotated.session.id, "s2");
    assert_eq!(rotated.route.actor_pid, created.route.actor_pid);
    assert_eq!(
        rotated.set_cookie_header,
        Some("terlan_session=s2; Path=/; HttpOnly; SameSite=Lax".to_string())
    );
    assert_eq!(
        sessions
            .read(&rotated.session, "role")
            .expect("rotated read should succeed"),
        Some(ReplValue::String("admin".to_string()))
    );
    assert_eq!(
        sessions
            .read(&created.session, "role")
            .expect_err("old id should be stale after rotation"),
        "stale HTTP session `s1`"
    );
}

#[test]
fn http_session_expiration_cleans_actor_table_and_reports_stale() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 2).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");
    sessions
        .write(
            &created.session,
            "user",
            ReplValue::String("ada".to_string()),
        )
        .expect("write should succeed");

    sessions.advance_ticks(2);
    assert_eq!(
        sessions.expire_due().expect("expiration should succeed"),
        vec!["s1".to_string()]
    );

    assert_eq!(sessions.snapshots(), Vec::new());
    assert_eq!(
        sessions
            .read(&created.session, "user")
            .expect_err("expired session should be stale"),
        "stale HTTP session `s1`"
    );

    let replacement = sessions
        .lookup_or_create(Some("s1"))
        .expect("expired cookie should create replacement");
    assert_eq!(replacement.session.id, "s2");
    assert_eq!(replacement.route.actor_pid, 2);
    assert_eq!(
        replacement.set_cookie_header,
        Some("terlan_session=s2; Path=/; HttpOnly; SameSite=Lax".to_string())
    );
}

#[test]
fn http_session_recovery_policy_can_fail_closed_for_stale_cookie() {
    let mut sessions = VmHttpSessionRuntime::new_with_recovery_policy(
        "node-a",
        1,
        VmHttpSessionRecoveryPolicy::FailClosed,
    )
    .expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");

    sessions.advance_ticks(1);
    let error = sessions
        .lookup_or_create(Some(&created.session.id))
        .expect_err("expired cookie should fail closed");

    assert_eq!(error, "stale HTTP session `s1`");
    assert_eq!(sessions.snapshots(), Vec::new());
}

#[test]
fn http_session_rejects_invalid_runtime_configuration() {
    assert_eq!(
        VmHttpSessionRuntime::new(" ", 10).expect_err("empty node should fail"),
        "HTTP session node id cannot be empty"
    );
    assert_eq!(
        VmHttpSessionRuntime::new("node-a", 0).expect_err("zero ttl should fail"),
        "HTTP session TTL must be greater than 0"
    );
}
