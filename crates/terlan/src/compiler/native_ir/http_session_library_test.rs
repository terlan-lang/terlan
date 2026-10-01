//! Session source behavior and native declarations use the ordinary compiler path.

#[test]
fn session_provider_bodies_and_colliding_local_calls_execute_as_source() {
    let provider = r#"
module std.http.Session.
pub struct Session { value: Int }.
pub current(value: Int): Session -> Session { value: value + 1 }.
pub (session: Session) get(key: Int): Int -> session.value + key.
pub (mut session: Session) set(value: Int): Unit -> Session { value: value }.
pub (mut session: Session) delete(key: Int): Unit -> Session { value: session.value - key }.
pub (mut session: Session) rotate(): Session -> Session { value: session.value + 100 }.
pub (mut session: Session) expire(): Unit -> Session { value: 0 }.
pub with_response(value: Int, session: Session): Int -> value + session.value.
"#;
    let caller = r#"
module session_source_authority.
import std.http.Session.
import std.http.Session.{get as read, with_response as attach}.
import std.core.Unit.
get(value: Int): Int -> value + 10.
set(value: Int): Int -> value + 20.
pub check(): Bool ->
    let session = Session.current(10);
    let snapshot = session;
    let done = session.set(30);
    session.delete(3);
    let before = read(session, 2) == 29 and session.get(1) == 28;
    let rotated = session.rotate();
    let result = attach(1, rotated) == 128;
    session.expire();
    done == Unit and snapshot.value == 11 and before and result
        and session.value == 0 and get(1) == 11 and set(1) == 21.
"#;
    super::super::source_constructor_test::check_sources(&[caller, provider]);
    super::super::source_constructor_test::check_sources(&[
        &caller.replace("std.http.Session", "app.Session"),
        &provider.replace("std.http.Session", "app.Session"),
    ]);
}

#[test]
fn session_cookie_selection_normalization_and_pending_identity_follow_source_bodies() {
    let request = r#"
module std.http.Request.
import std.core.Option.{None}.
pub struct Request { value: Option[String] }.
pub new(value: Option[String]): Request -> Request { value: value }.
pub (request: Request) cookie(name: String): Option[String] ->
    if { name == "terlan_session" -> request.value; true -> None }.
"#;
    let provider = include_str!("../../../../../std/http/Session.terl").replace(
        "@compiler.native {std.http.session.current}\nlookup(identity: String): String ->\n    native.",
        "lookup(identity: String): String -> if { identity == \"\" -> \"issued\"; true -> identity }.",
    ) + r#"
pub inspect(value: Option[String]): String ->
    let session = current(Request.new(value));
    session.#identity + ":" + session.#pending_identity.
"#;
    assert!(!provider.contains("@compiler.native {std.http.session.current}"));
    let caller = r#"
module session_cookie_source_policy.
import std.http.Session.
import std.core.Option.{Some, None}.
pub check(): Int ->
    if {
        Session.inspect(None) != "issued:issued" -> 11;
        Session.inspect(Some("")) != "issued:issued" -> 12;
        Session.inspect(Some("id")) != "id:" -> 13;
        Session.inspect(Some("  id  ")) != "id:" -> 14;
        Session.inspect(Some("\u{2003}id\u{2003}")) != "id:" -> 15;
        true -> 1
    }.
"#;
    let option = include_str!("../../../../../std/core/Option.terl");
    let string = include_str!("../../../../../std/core/String.terl");
    for owner in ["std.http.Session", "app.Session"] {
        let caller = caller.replace("std.http.Session", owner);
        let provider = provider.replace("std.http.Session", owner);
        eprintln!("session policy owner={owner} normalized");
        super::super::source_constructor_test::check_sources(&[
            &caller.replace("\\u{2003}", "\u{2003}"),
            &provider,
            request,
            option,
            string,
        ]);
        let untrimmed = caller
            .replace(
                "Some(\"  id  \")) != \"id:\"",
                "Some(\"  id  \")) != \"  id  :\"",
            )
            .replace(
                "Some(\"\\u{2003}id\\u{2003}\")) != \"id:\"",
                "Some(\"\\u{2003}id\\u{2003}\")) != \"\\u{2003}id\\u{2003}:\"",
            );
        eprintln!("session policy owner={owner} exact");
        super::super::source_constructor_test::check_sources(&[
            &untrimmed.replace("\\u{2003}", "\u{2003}"),
            &provider.replace("value.trim()", "value"),
            request,
            option,
            string,
        ]);
    }
}

#[test]
fn session_native_declarations_suspend_through_generic_capability_frames() {
    use super::super::source_constructor_test::checked_provider;
    use super::super::{NativeExpr, NativeModule, NativeTransitionOperation};

    for owner in ["std.http.Session", "app.Session"] {
        for (name, args, result) in [
            ("current", "identity: String", "String"),
            ("get", "identity: String, key: String", "Option[String]"),
            (
                "set",
                "identity: String, key: String, value: String",
                "Unit",
            ),
            ("delete", "identity: String, key: String", "Unit"),
            ("rotate", "identity: String", "String"),
            ("expire", "identity: String", "Unit"),
            ("is_live", "identity: String", "Bool"),
        ] {
            for prefix in ["std.http.session", "app.storage"] {
                let core = checked_provider(&format!(
                    "module {owner}. import std.core.Option. @compiler.native {{{prefix}.{name}}}
                     pub invoke({args}): {result} -> native."
                ));
                let modules = NativeModule::lower_application(&[&core]).unwrap();
                let function = modules
                    .iter()
                    .flat_map(|m| &m.functions)
                    .find(|f| f.name == "invoke")
                    .unwrap();
                assert!(
                    matches!(&function.body, NativeExpr::Suspend {
                    operation: NativeTransitionOperation::Capability,
                    arguments, ..
                } if matches!(arguments.first(), Some(NativeExpr::Int(7)))),
                    "{owner} {prefix}.{name}: {:?}",
                    function.body
                );
                let mut encodings = Vec::new();
                function.body.collect_managed_encodings(&mut encodings);
                assert!(encodings
                    .iter()
                    .all(|encoded| !encoded.starts_with(b"TVHS")));
                super::super::emit_native_application_object(owner, &modules).unwrap();
            }
        }
    }
}
