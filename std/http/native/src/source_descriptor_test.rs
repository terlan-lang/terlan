use super::*;
use terlan_runtime_abi::NativeValue as V;

fn rec(name: &str, fields: &[(&str, V)]) -> V {
    V::Record {
        name: name.into(),
        fields: fields
            .iter()
            .map(|(k, v)| ((*k).into(), v.clone()))
            .collect(),
    }
}
fn tag(name: &str, fields: Vec<V>) -> V {
    let (constructor, names): (_, &[&str]) = match name {
        "route" => ("Route", &["method", "path", "handler"]),
        "sse" => ("Sse", &["path", "endpoint"]),
        "websocket" => ("Websocket", &["path", "endpoint"]),
        "middleware" => ("Middleware", &["callback"]),
        "response_middleware" => ("Response_middleware", &["callback"]),
        "fallback" => ("Fallback", &["callback"]),
        "error" => ("Err", &["callback"]),
        "lifecycle" => ("Lifecycle", &["callback"]),
        "overload" => ("Overload", &["policy", "max_pending"]),
        "group_start" => ("Group_start", &["prefix"]),
        "callbacks" => (
            "Callbacks",
            &["open", "inbound", "writable", "close", "cancellation"],
        ),
        "pairing" => (
            "Pairing",
            &[
                "waiting",
                "first_matched",
                "second_matched",
                "peer_left",
                "inbound",
                "cancellation",
            ],
        ),
        "stateful_pairing" => (
            "Stateful_pairing",
            &[
                "waiting",
                "first_matched",
                "second_matched",
                "peer_left",
                "inbound",
                "cancellation",
            ],
        ),
        "restorable_pairing" => (
            "Restorable_pairing",
            &[
                "waiting",
                "peer_left",
                "room_query",
                "player_query",
                "room_prefix",
                "first_player",
                "second_player",
                "retention_ms",
                "retained_room_capacity",
                "matched",
                "restored",
                "inbound",
                "cancellation",
            ],
        ),
        _ => (name, &[]),
    };
    V::Record {
        name: constructor.into(),
        fields: names
            .iter()
            .zip(fields)
            .map(|(name, value)| ((*name).into(), value))
            .collect(),
    }
}
fn cb(id: i64, arity: i64) -> V {
    V::Tuple(vec![V::Int(id), V::Int(arity)])
}
fn admit(value: &V, arity: usize) -> Result<i64> {
    match value {
        V::Tuple(fields) => match fields.as_slice() {
            [V::Int(id), V::Int(actual)] if *actual == arity as i64 => Ok(*id),
            _ => Err(error("callback arity")),
        },
        _ => Err(error("callback type")),
    }
}
fn sse() -> V {
    rec(
        "Endpoint",
        &[
            ("max_pending_events", 4i64.into()),
            ("max_event_bytes", 1024i64.into()),
            ("keep_alive_ms", Some(50i64).into()),
            (
                "callbacks",
                V::List(vec![rec(
                    "Callbacks",
                    &[
                        ("open", cb(1, 0)),
                        ("event_ready", cb(2, 1)),
                        ("keep_alive", cb(3, 0)),
                        ("drain", cb(4, 0)),
                        ("cancellation", cb(5, 1)),
                    ],
                )]),
            ),
        ],
    )
}
fn websocket(policy: V) -> V {
    rec(
        "Endpoint",
        &[
            ("max_pending_frames", 3i64.into()),
            ("max_frame_bytes", 512i64.into()),
            ("policies", V::List(vec![policy])),
        ],
    )
}
fn router_value(entries: Vec<V>) -> V {
    rec("Router", &[("entries", V::List(entries))])
}
fn replace(value: &V, key: &str, replacement: V) -> V {
    let V::Record { name, fields } = value else {
        panic!("record")
    };
    V::Record {
        name: name.clone(),
        fields: fields
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    if k == key {
                        replacement.clone()
                    } else {
                        v.clone()
                    },
                )
            })
            .collect(),
    }
}

#[test]
fn executed_sse_records_preserve_limits_callback_order_and_optional_keep_alive() {
    let value = sse();
    let plan = sse_endpoint(&value, admit).unwrap();
    assert_eq!(plan.max_pending_events(), 4);
    assert_eq!(plan.max_event_bytes(), 1024);
    assert_eq!(plan.keep_alive_ms(), Some(50));
    let callbacks = plan.callbacks().unwrap();
    assert_eq!(
        [
            callbacks.open,
            callbacks.event_ready,
            callbacks.keep_alive,
            callbacks.drain,
            callbacks.cancellation
        ],
        [1, 2, 3, 4, 5]
    );
    for none in [None::<i64>.into(), V::Atom("none".into())] {
        let empty = replace(
            &replace(&value, "keep_alive_ms", none),
            "callbacks",
            V::List(vec![]),
        );
        let plan = sse_endpoint(&empty, admit).unwrap();
        assert_eq!(plan.keep_alive_ms(), None);
        assert!(plan.callbacks().is_none());
    }
}

#[test]
fn malformed_records_limits_options_and_callback_sets_are_rejected() {
    let value = sse();
    let V::Record { fields, .. } = &value else {
        panic!("record")
    };
    for field in 0..fields.len() {
        let mut missing = fields.clone();
        missing.remove(field);
        assert!(sse_endpoint(
            &V::Record {
                name: "Endpoint".into(),
                fields: missing
            },
            admit
        )
        .is_err());
        let mut duplicate = fields.clone();
        duplicate[field] = fields[(field + 1) % fields.len()].clone();
        assert!(sse_endpoint(
            &V::Record {
                name: "Endpoint".into(),
                fields: duplicate
            },
            admit
        )
        .is_err());
        assert!(sse_endpoint(&replace(&value, &fields[field].0, V::Unit), admit).is_err());
    }
    for limit in [0i64, -1] {
        for key in ["max_pending_events", "max_event_bytes"] {
            assert!(sse_endpoint(&replace(&value, key, limit.into()), admit).is_err());
        }
        assert!(
            sse_endpoint(&replace(&value, "keep_alive_ms", Some(limit).into()), admit).is_err()
        );
    }
    assert!(sse_endpoint(
        &replace(&value, "callbacks", V::List(vec![V::Unit, V::Unit])),
        admit
    )
    .is_err());
    assert!(sse_endpoint(
        &replace(
            &value,
            "keep_alive_ms",
            rec("Some", &[("wrong", 1i64.into())])
        ),
        admit
    )
    .is_err());
    let V::List(callbacks) = &fields[3].1 else {
        panic!("callbacks")
    };
    for (name, arity) in [
        ("open", 0),
        ("event_ready", 1),
        ("keep_alive", 0),
        ("drain", 0),
        ("cancellation", 1),
    ] {
        let invalid = replace(&callbacks[0], name, cb(1, arity + 1));
        assert!(
            sse_endpoint(&replace(&value, "callbacks", V::List(vec![invalid])), admit).is_err()
        );
    }
    assert!(sse_endpoint(&V::Unit, admit).is_err());
}

#[test]
fn websocket_policy_variants_preserve_callbacks_and_recovery_values() {
    let regular = tag(
        "callbacks",
        vec![cb(1, 0), cb(2, 1), cb(3, 0), cb(4, 0), cb(5, 1)],
    );
    let plan = websocket_endpoint(&websocket(regular.clone()), admit).unwrap();
    assert_eq!(plan.max_pending_frames(), 3);
    assert_eq!(plan.max_frame_bytes(), 512);
    assert_eq!(plan.callbacks().unwrap().inbound, 2);
    for (name, arity) in [("pairing", 1), ("stateful_pairing", 5)] {
        let policy = tag(
            name,
            vec![
                "waiting".into(),
                "first".into(),
                "second".into(),
                "left".into(),
                cb(6, arity),
                cb(7, 1),
            ],
        );
        let plan = websocket_endpoint(&websocket(policy), admit).unwrap();
        let pairing = plan.pairing().unwrap();
        assert_eq!(pairing.stateful, arity == 5);
        assert_eq!(pairing.waiting, "waiting");
        assert_eq!(pairing.first_matched, "first");
        assert_eq!(pairing.second_matched, "second");
        assert_eq!(pairing.peer_left, "left");
        assert_eq!((pairing.inbound, pairing.cancellation), (6, 7));
        assert!(pairing.restoration.is_none());
    }
    let restoration = tag(
        "restorable_pairing",
        vec![
            cb(1, 0),
            cb(2, 0),
            "room".into(),
            "player".into(),
            "room-".into(),
            "one".into(),
            "two".into(),
            1000i64.into(),
            4i64.into(),
            cb(3, 4),
            cb(4, 5),
            cb(5, 5),
            cb(6, 1),
        ],
    );
    let plan = websocket_endpoint(&websocket(restoration.clone()), admit).unwrap();
    let pairing = plan.pairing().unwrap();
    assert!(pairing.stateful);
    let recovery = pairing.restoration.as_ref().unwrap();
    assert_eq!(
        (
            recovery.waiting,
            recovery.peer_left,
            recovery.matched,
            recovery.restored
        ),
        (1, 2, 3, 4)
    );
    assert_eq!(recovery.room_query, "room");
    assert_eq!(recovery.player_query, "player");
    assert_eq!(recovery.room_prefix, "room-");
    assert_eq!(
        (recovery.retention_ms, recovery.retained_room_capacity),
        (1000, 4)
    );
    for policy in [regular, restoration] {
        let V::Record { name, fields } = &policy else {
            panic!("policy")
        };
        for index in 0..fields.len() {
            let mut fields = fields.clone();
            fields[index].1 = V::Unit;
            assert!(websocket_endpoint(
                &websocket(V::Record {
                    name: name.clone(),
                    fields
                }),
                admit
            )
            .is_err());
        }
        let duplicate = replace(
            &websocket(policy.clone()),
            "policies",
            V::List(vec![policy.clone(), policy]),
        );
        assert!(websocket_endpoint(&duplicate, admit).is_err());
    }
    let empty = replace(&websocket(V::Unit), "policies", V::List(vec![]));
    assert!(websocket_endpoint(&empty, admit)
        .unwrap()
        .callbacks()
        .is_none());
    assert!(websocket_endpoint(&websocket(tag("unknown", vec![])), admit).is_err());
}

#[test]
fn router_groups_preserve_global_and_scoped_middleware_and_channel_targets() {
    let value = router_value(vec![
        tag("middleware", vec![cb(1, 1)]),
        tag("response_middleware", vec![cb(2, 2)]),
        tag("group_start", vec!["/api".into()]),
        tag("middleware", vec![cb(3, 1)]),
        tag("response_middleware", vec![cb(4, 2)]),
        tag("route", vec!["GET".into(), "/users".into(), cb(5, 1)]),
        tag("fallback", vec![cb(6, 1)]),
        tag("error", vec![cb(7, 1)]),
        V::Atom("group_end_entry".into()),
        tag("sse", vec!["/events".into(), sse()]),
        tag(
            "websocket",
            vec![
                "/socket".into(),
                replace(&websocket(V::Unit), "policies", V::List(vec![])),
            ],
        ),
        tag("fallback", vec![cb(8, 1)]),
    ]);
    let plan = router(&value, admit).unwrap();
    assert_eq!(plan.middleware, [1]);
    assert_eq!(plan.response_middleware, [2]);
    assert_eq!(plan.routes.len(), 10);
    assert_eq!(plan.routes[0].path, "/api/users");
    assert_eq!(plan.routes[0].middleware, [3]);
    assert_eq!(plan.routes[0].response_middleware, [4]);
    assert!(matches!(plan.routes[0].target, RouteTarget::Handler(5)));
    assert_eq!(plan.routes[1].path, "/api/*");
    assert!(matches!(plan.routes[8].target, RouteTarget::Sse(_)));
    assert!(matches!(plan.routes[9].target, RouteTarget::WebSocket(_)));
    assert_eq!(plan.fallback, Some(8));
    assert_eq!(plan.error, Some(7));
}

#[test]
fn invalid_router_shapes_scopes_and_duplicate_singletons_fail_closed() {
    for entries in [
        vec![V::Atom("group_end_entry".into())],
        vec![tag("group_start", vec!["/x".into()])],
        vec![tag("route", vec![])],
        vec![V::Tuple(vec![])],
        vec![V::Unit],
        vec![tag("unknown", vec![])],
        vec![tag(
            "overload",
            vec![V::Atom("unknown".into()), 1i64.into()],
        )],
        vec![tag("overload", vec![V::Atom("queue".into()), 0i64.into()])],
    ] {
        assert!(router(&router_value(entries), admit).is_err());
    }
    for tag_name in ["fallback", "error", "lifecycle"] {
        let entry = tag(tag_name, vec![cb(1, 1)]);
        assert!(router(&router_value(vec![entry.clone(), entry]), admit).is_err());
    }
    let overload = tag("overload", vec![V::Atom("reject".into()), 3i64.into()]);
    let plan = router(
        &router_value(vec![overload.clone(), tag("lifecycle", vec![cb(9, 1)])]),
        admit,
    )
    .unwrap();
    assert_eq!(plan.overload, Some(("reject".into(), 3)));
    assert_eq!(plan.lifecycle, Some(9));
    assert!(router(
        &router_value(vec![overload.clone(), overload.clone()]),
        admit
    )
    .is_err());
    assert!(router(
        &router_value(vec![
            tag("group_start", vec!["/x".into()]),
            overload,
            V::Atom("group_end_entry".into())
        ]),
        admit
    )
    .is_err());
    let nested = vec![tag("group_start", vec!["/x".into()]); 64];
    assert!(router(&router_value(nested), admit).is_err());
}

#[test]
fn constructor_field_identity_is_checked_independently_of_storage_order() {
    let entry = tag("route", vec!["GET".into(), "/checked".into(), cb(1, 1)]);
    let V::Record { name, fields } = entry else {
        panic!("constructor")
    };
    let mut reversed = fields.clone();
    reversed.reverse();
    let plan = router(
        &router_value(vec![V::Record {
            name: name.clone(),
            fields: reversed,
        }]),
        admit,
    )
    .unwrap();
    assert_eq!(plan.routes[0].method, "GET");
    assert_eq!(plan.routes[0].path, "/checked");
    for index in 0..fields.len() {
        let mut wrong = fields.clone();
        wrong[index].0 = "unknown".into();
        assert!(router(
            &router_value(vec![V::Record {
                name: name.clone(),
                fields: wrong
            }]),
            admit
        )
        .is_err());
        let mut duplicate = fields.clone();
        duplicate[index] = fields[(index + 1) % fields.len()].clone();
        assert!(router(
            &router_value(vec![V::Record {
                name: name.clone(),
                fields: duplicate
            }]),
            admit
        )
        .is_err());
    }
    for invalid in [
        V::Tuple(vec![
            V::Atom("route".into()),
            "GET".into(),
            "/checked".into(),
            cb(1, 1),
        ]),
        rec("Group_end_entry", &[("extra", V::Unit)]),
        V::Atom("group_end".into()),
    ] {
        assert!(router(&router_value(vec![invalid]), admit).is_err());
    }
    let nested = router_value(vec![
        tag("group_start", vec!["/outer".into()]),
        tag("group_start", vec!["/inner".into()]),
        tag("route", vec!["GET".into(), "/".into(), cb(1, 1)]),
        rec("Group_end_entry", &[]),
        rec("Group_end_entry", &[]),
    ]);
    assert_eq!(
        router(&nested, admit).unwrap().routes[0].path,
        "/outer/inner"
    );
}
