use std::io::Cursor;

use super::live_template_repeated_diff::{
    build_live_template_repeated_diff, VmHttpSessionLiveTemplateRenderedFragment,
    VmHttpSessionLiveTemplateRepeatedBinding, VmHttpSessionLiveTemplateRepeatedPatch,
};
use super::live_template_response::{
    VmHttpSessionLiveTemplateRenderPlan, VmHttpSessionLiveTemplateStreamPlan,
};
use super::{VmHttpSessionLiveTemplateSourceSpan, VmHttpSessionRuntime};
use crate::runtime::vm::framing::VmInMemoryFrameReader;
use crate::runtime::vm::http::handle_http1_in_memory_exchange;
use crate::runtime::vm::http_router::{VmHttpRouteMethod, VmHttpRouter, VmHttpRouterOutcome};
use crate::runtime::vm::http_static::{VmHttp1ResponseStream, VmHttp1StreamTcpFlush};
use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::tcp::VmTcpRuntime;
use crate::runtime::vm::ReplValue;

fn source() -> VmHttpSessionLiveTemplateSourceSpan {
    VmHttpSessionLiveTemplateSourceSpan::new("app.UserPage", 8, 17)
        .expect("live-template source span")
}

fn repeated_user(key: &str, id: i64, name: &str) -> VmHttpSessionLiveTemplateRepeatedBinding {
    VmHttpSessionLiveTemplateRepeatedBinding::new(
        key,
        ReplValue::Tuple(vec![
            ReplValue::Int(id),
            ReplValue::String(name.to_string()),
        ]),
    )
}

fn render_user_fragment(value: &ReplValue) -> Result<String, String> {
    let ReplValue::Tuple(fields) = value else {
        return Err("expected user tuple".to_string());
    };
    let [ReplValue::Int(id), ReplValue::String(name)] = fields.as_slice() else {
        return Err("expected {Int, String} user tuple".to_string());
    };
    Ok(format!("<li data-id=\"{id}\">{name}</li>"))
}

fn render_user_list(value: &ReplValue) -> Result<String, String> {
    let ReplValue::List(users) = value else {
        return Err("expected user list".to_string());
    };
    let items = users
        .iter()
        .map(render_user_fragment)
        .collect::<Result<String, _>>()?;
    Ok(format!("<ul>{items}</ul>"))
}

fn rendered_users(
    bindings: &[VmHttpSessionLiveTemplateRepeatedBinding],
) -> Vec<VmHttpSessionLiveTemplateRenderedFragment> {
    bindings
        .iter()
        .map(|binding| VmHttpSessionLiveTemplateRenderedFragment {
            key: binding.key.clone(),
            content: render_user_fragment(&binding.value).expect("valid user fixture"),
        })
        .collect()
}

fn apply_repeated_patches(
    mut fragments: Vec<VmHttpSessionLiveTemplateRenderedFragment>,
    patches: &[VmHttpSessionLiveTemplateRepeatedPatch],
) -> Vec<VmHttpSessionLiveTemplateRenderedFragment> {
    for patch in patches {
        match patch {
            VmHttpSessionLiveTemplateRepeatedPatch::Insert {
                index,
                key,
                content,
            } => fragments.insert(
                *index,
                VmHttpSessionLiveTemplateRenderedFragment {
                    key: key.clone(),
                    content: content.clone(),
                },
            ),
            VmHttpSessionLiveTemplateRepeatedPatch::Remove { index, key } => {
                assert_eq!(&fragments[*index].key, key);
                fragments.remove(*index);
            }
            VmHttpSessionLiveTemplateRepeatedPatch::Move { from, to, key } => {
                assert_eq!(&fragments[*from].key, key);
                let fragment = fragments.remove(*from);
                fragments.insert(*to, fragment);
            }
            VmHttpSessionLiveTemplateRepeatedPatch::Replace {
                index,
                key,
                content,
            } => {
                assert_eq!(&fragments[*index].key, key);
                fragments[*index].content = content.clone();
            }
        }
    }
    fragments
}

fn collect_live_template_stream_wire(mut stream: VmHttp1ResponseStream) -> String {
    let mut tcp = VmTcpRuntime::new();
    let listener = tcp.listen("live-template-stream-test").expect("listen");
    let client = tcp
        .connect("live-template-stream-test", "client")
        .expect("connect");
    let server = tcp
        .accept(listener, "server")
        .expect("accept")
        .expect("server stream");
    tcp.set_stream_inbox_limit(client, 16 * 1024)
        .expect("set peer inbox limit");
    let mut writer = VmInMemoryFrameReader::new(server, 4096).expect("writer");
    let process = VmProcessId::from_raw_for_test(83);
    let mut wire = Vec::new();

    loop {
        match stream
            .flush_next_to_tcp(&mut writer, &mut tcp, process)
            .expect("flush live-template stream")
        {
            VmHttp1StreamTcpFlush::Written { .. } => wire.extend(
                tcp.receive(client, 16 * 1024)
                    .expect("receive wire bytes")
                    .expect("written stream part"),
            ),
            VmHttp1StreamTcpFlush::Complete => break,
            VmHttp1StreamTcpFlush::Idle | VmHttp1StreamTcpFlush::Parked { .. } => {
                panic!("unpressured live-template stream should make progress")
            }
        }
    }
    String::from_utf8(wire).expect("HTTP/1 stream should be UTF-8")
}

fn decode_chunked_body(wire: &str) -> String {
    let (_, mut body) = wire
        .split_once("\r\n\r\n")
        .expect("HTTP/1 response head separator");
    let mut decoded = String::new();
    loop {
        let (size, remainder) = body.split_once("\r\n").expect("chunk size line");
        let size = usize::from_str_radix(size, 16).expect("hex chunk size");
        body = remainder;
        if size == 0 {
            assert_eq!(body, "\r\n");
            return decoded;
        }
        let (chunk, remainder) = body.split_at(size);
        decoded.push_str(chunk);
        body = remainder
            .strip_prefix("\r\n")
            .expect("chunk terminator before next size");
    }
}

#[test]
fn http_handler_renders_actor_updated_live_template_state() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");
    sessions
        .subscribe_live_template(&created.session, "user-page", "sse")
        .expect("subscribe live template");
    let router = VmHttpRouter::new()
        .get("/api/{id}", ReplValue::Atom("render_user_page".to_string()))
        .expect("register parameterized route");
    let request = b"GET /api/42 HTTP/1.1\r\nHost: vm.local\r\nContent-Length: 0\r\n\r\n";
    let mut reader = Cursor::new(request.as_slice());
    let mut writer = Vec::new();

    let exchange = handle_http1_in_memory_exchange(&mut reader, &mut writer, false, |request| {
        let outcome = router.dispatch(VmHttpRouteMethod::Get, request.uri().path())?;
        let VmHttpRouterOutcome::Matched(dispatch) = outcome else {
            return Err("expected /api/{id} route to match".to_string());
        };
        let user_id = dispatch
            .route_params
            .iter()
            .find_map(|(name, value)| (name == "id").then_some(value))
            .ok_or_else(|| "missing id route parameter".to_string())?
            .parse::<i64>()
            .map_err(|_| "invalid id route parameter".to_string())?;
        sessions.dispatch_live_template_command_to_actor_mailbox(
            &created.session,
            "load-user-42",
            "load_user",
            ReplValue::Int(user_id),
        )?;
        let command = sessions
            .receive_live_template_actor_command(&created.session)?
            .ok_or_else(|| "expected live-template actor command".to_string())?;
        assert_eq!(command.command_id, "load-user-42");
        assert_eq!(command.name, "load_user");
        let ReplValue::Int(actor_user_id) = command.body else {
            return Err("expected Int actor command body".to_string());
        };
        let observed_version = sessions.state_version(&created.session)?;
        let fanout = sessions.fanout_live_template_state_update(
            &created.session,
            observed_version,
            "user.patch",
            &source(),
            ReplValue::Int(actor_user_id),
            |runtime, session| runtime.write(session, "user_id", ReplValue::Int(actor_user_id)),
        )?;
        let rendered = sessions.render_live_template_actor_state_response(
            &created.session,
            VmHttpSessionLiveTemplateRenderPlan {
                template_id: "user.page",
                state_key: "user_id",
                template_name: "UserPage",
                source_file: "templates/user_page.terl.html",
                source: &source(),
                status: http::StatusCode::OK,
            },
            |value| match value {
                ReplValue::Int(id) => Ok(format!(
                    "<article data-user-id=\"{id}\">User {id}</article>"
                )),
                other => Err(format!("expected Int, received {other:?}")),
            },
        )?;
        assert_eq!(rendered.binding.state_version, fanout.state_version);
        assert_eq!(rendered.binding.state_value, Some(ReplValue::Int(42)));
        assert_eq!(fanout.subscriber_events.len(), 1);
        Ok(rendered.response)
    })
    .expect("actor-bound template handler should complete");

    assert_eq!(exchange.request_path, "/api/42");
    assert_eq!(exchange.response_status, 200);
    let response = String::from_utf8(writer).expect("response should be UTF-8");
    assert!(response.contains("content-type: text/html; charset=utf-8\r\n"));
    assert!(response.contains("x-terlan-template: UserPage\r\n"));
    assert!(response.ends_with("\r\n\r\n<article data-user-id=\"42\">User 42</article>"));
    assert_eq!(sessions.snapshots()[0].actor_mailbox_len, 0);
}

#[test]
fn actor_bound_template_response_rejects_missing_and_opaque_state_with_source_span() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");
    let source = source();
    let plan = VmHttpSessionLiveTemplateRenderPlan {
        template_id: "user.page",
        state_key: "user_id",
        template_name: "UserPage",
        source_file: "templates/user_page.terl.html",
        source: &source,
        status: http::StatusCode::OK,
    };

    let missing = sessions
        .render_live_template_actor_state_response(&created.session, plan, |_| {
            panic!("missing actor state must fail before rendering")
        })
        .expect_err("missing actor state should fail");
    assert_eq!(
        missing,
        "template_runtime_actor_bind_error: app.UserPage:8:17: HTTP live-template `user.page` actor state `user_id` is unavailable"
    );

    sessions
        .write(
            &created.session,
            "user_id",
            ReplValue::Bytes(vec![1, 2, 3].into()),
        )
        .expect("write opaque actor state");
    let opaque = sessions
        .render_live_template_actor_state_response(&created.session, plan, |_| {
            panic!("opaque actor state must fail before rendering")
        })
        .expect_err("opaque actor state should fail");
    assert_eq!(
        opaque,
        "invalid_template_actor_return_type: app.UserPage:8:17: HTTP live-template actor return type Bytes cannot be serialized as a typed patch payload"
    );
}

#[test]
fn actor_bound_template_stream_writes_typed_chunks_through_vm_tcp() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");
    let users = ReplValue::List(vec![
        ReplValue::Tuple(vec![
            ReplValue::Int(1),
            ReplValue::String("Ada".to_string()),
        ]),
        ReplValue::Tuple(vec![
            ReplValue::Int(2),
            ReplValue::String("Bert".to_string()),
        ]),
    ]);
    sessions
        .write(&created.session, "users", users.clone())
        .expect("write actor state");
    let expected_version = sessions
        .state_version(&created.session)
        .expect("state version");
    let source = source();
    let mut open = sessions
        .open_live_template_actor_state_stream(
            &created.session,
            VmHttpSessionLiveTemplateStreamPlan {
                response: VmHttpSessionLiveTemplateRenderPlan {
                    template_id: "users.stream",
                    state_key: "users",
                    template_name: "UserStream",
                    source_file: "templates/user_stream.terl.html",
                    source: &source,
                    status: http::StatusCode::OK,
                },
                chunk_size: 16,
                max_pending_writes: 16,
                close_connection: true,
            },
        )
        .expect("open actor-bound template stream");

    assert_eq!(
        open.enqueue_rendered_chunk(|_| Ok("<ul>".to_string())),
        Ok(1)
    );
    for index in 0..2 {
        assert_eq!(
            open.enqueue_rendered_chunk(|state| {
                let ReplValue::List(users) = state else {
                    return Err("expected user list".to_string());
                };
                render_user_fragment(&users[index])
            }),
            Ok(2)
        );
    }
    assert_eq!(
        open.enqueue_rendered_chunk(|_| Ok("</ul>".to_string())),
        Ok(1)
    );
    let rendered = open.finish().expect("finish template stream");

    assert_eq!(rendered.binding.state_key, "users");
    assert_eq!(rendered.binding.state_version, expected_version);
    assert_eq!(rendered.binding.state_value, Some(users));
    let wire = collect_live_template_stream_wire(rendered.stream);
    let (head, _) = wire.split_once("\r\n\r\n").expect("response head");
    assert!(head.starts_with("HTTP/1.1 200 OK\r\n"));
    let normalized_head = head.to_ascii_lowercase();
    assert!(normalized_head.contains("content-type: text/html; charset=utf-8"));
    assert!(normalized_head.contains("x-terlan-template: userstream"));
    assert!(normalized_head.contains("transfer-encoding: chunked"));
    assert!(normalized_head.contains("connection: close"));
    assert!(!normalized_head.contains("content-length:"));
    assert_eq!(
        decode_chunked_body(&wire),
        "<ul><li data-id=\"1\">Ada</li><li data-id=\"2\">Bert</li></ul>"
    );
}

#[test]
fn actor_bound_template_stream_rejects_partial_admission_and_supports_abort() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");
    sessions
        .write(
            &created.session,
            "content",
            ReplValue::String("actor-owned".to_string()),
        )
        .expect("write actor state");
    let state_version = sessions
        .state_version(&created.session)
        .expect("state version");
    let source = source();
    let render_plan = VmHttpSessionLiveTemplateRenderPlan {
        template_id: "content.stream",
        state_key: "content",
        template_name: "ContentStream",
        source_file: "templates/content_stream.terl.html",
        source: &source,
        status: http::StatusCode::OK,
    };

    let invalid_limit = sessions
        .open_live_template_actor_state_stream(
            &created.session,
            VmHttpSessionLiveTemplateStreamPlan {
                response: render_plan,
                chunk_size: 0,
                max_pending_writes: 1,
                close_connection: false,
            },
        )
        .expect_err("zero chunk size must fail");
    assert_eq!(
        invalid_limit,
        "template_runtime_unavailable: app.UserPage:8:17: HTTP live-template `content.stream` stream InvalidStreamLimit"
    );

    let mut open = sessions
        .open_live_template_actor_state_stream(
            &created.session,
            VmHttpSessionLiveTemplateStreamPlan {
                response: render_plan,
                chunk_size: 4,
                max_pending_writes: 1,
                close_connection: false,
            },
        )
        .expect("open bounded stream");
    assert_eq!(
        open.enqueue_rendered_chunk(|_| Err("renderer unavailable".to_string())),
        Err("template_runtime_actor_bind_error: app.UserPage:8:17: HTTP live-template `content.stream` stream render failed: renderer unavailable".to_string())
    );
    assert_eq!(
        open.enqueue_rendered_chunk(|_| Ok(String::new())),
        Err("template_runtime_unavailable: app.UserPage:8:17: HTTP live-template `content.stream` stream InvalidStreamChunk".to_string())
    );
    assert_eq!(
        open.enqueue_rendered_chunk(|_| Ok("abcdefgh".to_string())),
        Err("template_runtime_unavailable: app.UserPage:8:17: HTTP live-template `content.stream` stream StreamBackpressure".to_string())
    );
    assert_eq!(
        open.enqueue_rendered_chunk(|_| Ok("abcd".to_string())),
        Ok(1)
    );
    assert_eq!(
        open.enqueue_rendered_chunk(|_| Ok("x".to_string())),
        Err("template_runtime_unavailable: app.UserPage:8:17: HTTP live-template `content.stream` stream StreamBackpressure".to_string())
    );
    let rendered = open.finish().expect("finish bounded stream");
    assert_eq!(
        decode_chunked_body(&collect_live_template_stream_wire(rendered.stream)),
        "abcd"
    );

    let mut aborted = sessions
        .open_live_template_actor_state_stream(
            &created.session,
            VmHttpSessionLiveTemplateStreamPlan {
                response: render_plan,
                chunk_size: 4,
                max_pending_writes: 2,
                close_connection: false,
            },
        )
        .expect("open abortable stream");
    assert_eq!(
        aborted.enqueue_rendered_chunk(|_| Ok("abcdef".to_string())),
        Ok(2)
    );
    assert_eq!(aborted.abort(), Ok(2));
    assert_eq!(
        sessions
            .state_version(&created.session)
            .expect("state version after rejected stream operations"),
        state_version
    );
}

#[test]
fn live_template_actor_command_receive_rejects_malformed_mailbox_message() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");
    sessions
        .enqueue_actor_message(
            &created.session,
            ReplValue::Tuple(vec![
                ReplValue::Atom("wrong_command".to_string()),
                ReplValue::String("command-1".to_string()),
                ReplValue::String("load_user".to_string()),
                ReplValue::Int(42),
            ]),
        )
        .expect("enqueue malformed command");

    assert_eq!(
        sessions
            .receive_live_template_actor_command(&created.session)
            .expect_err("malformed command should fail"),
        "invalid_live_template_actor_command: session actor mailbox message must be {live_template_command, command_id, name, body}"
    );
    assert_eq!(sessions.snapshots()[0].actor_mailbox_len, 0);
    assert_eq!(
        sessions
            .receive_live_template_actor_command(&created.session)
            .expect("empty mailbox receive should succeed"),
        None
    );
}

#[test]
fn repeated_actor_interpolation_renders_with_deterministic_keyed_diff() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");
    sessions
        .subscribe_live_template(&created.session, "user-list", "sse")
        .expect("subscribe live template");
    let previous = vec![
        repeated_user("user-1", 1, "Ada"),
        repeated_user("user-2", 2, "Bert"),
        repeated_user("user-3", 3, "Cleo"),
        repeated_user("user-4", 4, "Dara"),
    ];
    let current = vec![
        repeated_user("user-2", 2, "Bert"),
        repeated_user("user-1", 1, "Ada Lovelace"),
        repeated_user("user-5", 5, "Eve"),
    ];
    let expected_operations = vec![
        VmHttpSessionLiveTemplateRepeatedPatch::Move {
            from: 1,
            to: 0,
            key: "user-2".to_string(),
        },
        VmHttpSessionLiveTemplateRepeatedPatch::Replace {
            index: 1,
            key: "user-1".to_string(),
            content: "<li data-id=\"1\">Ada Lovelace</li>".to_string(),
        },
        VmHttpSessionLiveTemplateRepeatedPatch::Insert {
            index: 2,
            key: "user-5".to_string(),
            content: "<li data-id=\"5\">Eve</li>".to_string(),
        },
        VmHttpSessionLiveTemplateRepeatedPatch::Remove {
            index: 4,
            key: "user-4".to_string(),
        },
        VmHttpSessionLiveTemplateRepeatedPatch::Remove {
            index: 3,
            key: "user-3".to_string(),
        },
    ];
    let observed_version = sessions
        .state_version(&created.session)
        .expect("state version should read");
    let current_state = ReplValue::List(
        current
            .iter()
            .map(|binding| binding.value.clone())
            .collect(),
    );

    let repeated = sessions
        .fanout_live_template_repeated_state_update(
            &created.session,
            observed_version,
            "users.patch",
            &source(),
            &previous,
            &current,
            render_user_fragment,
            |runtime, session| runtime.write(session, "users", current_state),
        )
        .expect("repeated actor update should fan out");
    assert_eq!(repeated.diff.operations, expected_operations);
    assert_eq!(repeated.fanout.state_version, observed_version + 1);
    assert_eq!(repeated.fanout.subscriber_events.len(), 1);
    assert_eq!(
        repeated.fanout.subscriber_events[0].payload,
        ReplValue::Tuple(vec![
            ReplValue::Atom("live_template_state_update".to_string()),
            ReplValue::String("users.patch".to_string()),
            ReplValue::Int(1),
            ReplValue::Tuple(vec![
                ReplValue::Atom("repeated_fragment_diff".to_string()),
                ReplValue::List(vec![
                    ReplValue::Tuple(vec![
                        ReplValue::Atom("move".to_string()),
                        ReplValue::Int(1),
                        ReplValue::Int(0),
                        ReplValue::String("user-2".to_string()),
                    ]),
                    ReplValue::Tuple(vec![
                        ReplValue::Atom("replace".to_string()),
                        ReplValue::Int(1),
                        ReplValue::String("user-1".to_string()),
                        ReplValue::String("<li data-id=\"1\">Ada Lovelace</li>".to_string(),),
                    ]),
                    ReplValue::Tuple(vec![
                        ReplValue::Atom("insert".to_string()),
                        ReplValue::Int(2),
                        ReplValue::String("user-5".to_string()),
                        ReplValue::String("<li data-id=\"5\">Eve</li>".to_string()),
                    ]),
                    ReplValue::Tuple(vec![
                        ReplValue::Atom("remove".to_string()),
                        ReplValue::Int(4),
                        ReplValue::String("user-4".to_string()),
                    ]),
                    ReplValue::Tuple(vec![
                        ReplValue::Atom("remove".to_string()),
                        ReplValue::Int(3),
                        ReplValue::String("user-3".to_string()),
                    ]),
                ]),
            ]),
        ])
    );

    let rendered = sessions
        .render_live_template_actor_state_response(
            &created.session,
            VmHttpSessionLiveTemplateRenderPlan {
                template_id: "users.list",
                state_key: "users",
                template_name: "UserList",
                source_file: "templates/user_list.terl.html",
                source: &source(),
                status: http::StatusCode::OK,
            },
            render_user_list,
        )
        .expect("updated repeated state should render");
    assert_eq!(
        rendered.binding.state_version,
        repeated.fanout.state_version
    );
    assert_eq!(
        rendered.response.body(),
        "<ul><li data-id=\"2\">Bert</li><li data-id=\"1\">Ada Lovelace</li><li data-id=\"5\">Eve</li></ul>"
    );

    let repeated_again =
        build_live_template_repeated_diff(&previous, &current, &source(), render_user_fragment)
            .expect("same repeated inputs should diff again");
    assert_eq!(repeated_again, repeated.diff);
}

#[test]
fn repeated_actor_diff_rejects_invalid_keys_and_render_failure_before_update() {
    let mut sessions =
        VmHttpSessionRuntime::new("node-a", 10).expect("session runtime should create");
    let created = sessions.lookup_or_create(None).expect("create session");
    let observed_version = sessions
        .state_version(&created.session)
        .expect("state version should read");

    for (side, previous, current, expected_detail) in [
        (
            "previous",
            vec![
                repeated_user("same", 1, "Ada"),
                repeated_user("same", 2, "Bert"),
            ],
            Vec::new(),
            "previous fragment key `same` is duplicated",
        ),
        (
            "current",
            Vec::new(),
            vec![
                repeated_user("same", 1, "Ada"),
                repeated_user("same", 2, "Bert"),
            ],
            "current fragment key `same` is duplicated",
        ),
    ] {
        let mut update_ran = false;
        let error = sessions
            .fanout_live_template_repeated_state_update(
                &created.session,
                observed_version,
                "users.patch",
                &source(),
                &previous,
                &current,
                |_| panic!("duplicate {side} keys must fail before rendering"),
                |_, _| {
                    update_ran = true;
                    Ok(())
                },
            )
            .expect_err("duplicate repeated keys should fail");
        assert_eq!(
            error,
            format!(
                "template_runtime_actor_bind_error: app.UserPage:8:17: HTTP live-template repeated fragment {expected_detail}"
            )
        );
        assert!(!update_ran);
    }

    for key in ["", " padded", "line\nbreak"] {
        let error = build_live_template_repeated_diff(
            &[],
            &[repeated_user(key, 1, "Ada")],
            &source(),
            |_| panic!("invalid keys must fail before rendering"),
        )
        .expect_err("invalid repeated key should fail");
        assert_eq!(
            error,
            "template_runtime_actor_bind_error: app.UserPage:8:17: HTTP live-template repeated fragment current fragment key must be non-empty, normalized, and control-free"
        );
    }

    let mut update_ran = false;
    let render_error = sessions
        .fanout_live_template_repeated_state_update(
            &created.session,
            observed_version,
            "users.patch",
            &source(),
            &[],
            &[repeated_user("user-1", 1, "Ada")],
            |_| Err("renderer unavailable".to_string()),
            |_, _| {
                update_ran = true;
                Ok(())
            },
        )
        .expect_err("render failure should reject repeated update");
    assert_eq!(
        render_error,
        "template_runtime_actor_bind_error: app.UserPage:8:17: HTTP live-template repeated fragment fragment `user-1` render failed: renderer unavailable"
    );
    assert!(!update_ran);
    assert_eq!(
        sessions
            .state_version(&created.session)
            .expect("state version should remain readable"),
        observed_version
    );
}

#[test]
fn repeated_fragment_diff_reconstructs_every_small_adversarial_transition() {
    let variants = vec![
        vec![],
        vec![repeated_user("a", 1, "Ada")],
        vec![repeated_user("b", 2, "Bert")],
        vec![repeated_user("a", 1, "Ada"), repeated_user("b", 2, "Bert")],
        vec![repeated_user("b", 2, "Bert"), repeated_user("a", 1, "Ada")],
        vec![
            repeated_user("a", 1, "Ada"),
            repeated_user("b", 2, "Bert"),
            repeated_user("c", 3, "Cleo"),
        ],
        vec![repeated_user("c", 3, "Cleo"), repeated_user("a", 1, "Ada")],
        vec![
            repeated_user("c", 3, "Cleo"),
            repeated_user("b", 2, "Bert"),
            repeated_user("a", 1, "Ada"),
        ],
        vec![
            repeated_user("a", 1, "Ada Lovelace"),
            repeated_user("b", 2, "Bert"),
        ],
    ];

    for previous in &variants {
        for current in &variants {
            let mut rendered_count = 0;
            let diff = build_live_template_repeated_diff(previous, current, &source(), |value| {
                rendered_count += 1;
                render_user_fragment(value)
            })
            .expect("valid repeated transition should diff");
            assert_eq!(rendered_count, previous.len() + current.len());
            assert_eq!(
                apply_repeated_patches(rendered_users(previous), &diff.operations),
                rendered_users(current)
            );

            let repeated = build_live_template_repeated_diff(
                previous,
                current,
                &source(),
                render_user_fragment,
            )
            .expect("same repeated transition should diff again");
            assert_eq!(repeated, diff);
        }
    }
}
