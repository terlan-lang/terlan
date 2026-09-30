//! Session imports cannot override source calls or unrelated receiver methods.

use super::*;
use crate::terlan_typeck::{CoreIntrinsicCall, CoreIntrinsicId, CoreType};

pub(super) fn primitive(name: &str, arity: usize) -> CoreExpr {
    CoreExpr::Intrinsic(CoreIntrinsicCall {
        id: CoreIntrinsicId::NativeOperation {
            operation: format!("std.http.session.{name}"),
            parameter_types: vec![CoreType::String; arity],
        },
        args: (0..arity)
            .map(|index| CoreExpr::Var(format!("arg{index}")))
            .collect(),
        return_type: match name {
            "current" => CoreType::Tuple(vec![
                crate::terlan_typeck::CoreTupleTypeElem::Type(CoreType::String),
                crate::terlan_typeck::CoreTupleTypeElem::Type(CoreType::Bool),
            ]),
            "rotate" => CoreType::String,
            "is_live" => CoreType::Bool,
            "get" => CoreType::Apply {
                constructor: "Option".into(),
                args: vec![CoreType::String],
            },
            _ => CoreType::Named("Unit".into()),
        },
        effects: CoreEffectSet { effects: vec![] },
        span: crate::terlan_syntax::span::Span::new(0, 0),
    })
}

#[test]
fn explicit_session_primitives_lower_without_import_or_provider_name_magic() {
    for (name, arity) in [
        ("current", 1),
        ("get", 2),
        ("set", 3),
        ("delete", 2),
        ("rotate", 1),
        ("expire", 1),
        ("is_live", 1),
    ] {
        let mut core = http_core();
        core.imports.clear();
        *body(&mut core) = primitive(name, arity);
        let layouts = http_managed_layouts(&core).unwrap();
        assert!(!layouts.is_empty(), "{name}");
        assert_eq!(
            layouts.len(),
            3,
            "session metadata must only install option and lookup-pair layouts"
        );

        lower_http_values(&mut core).unwrap();
        assert_eq!(http_managed_layouts(&core).unwrap(), layouts);
        let operation = body(&mut core);
        assert!(
            matches!(operation, CoreExpr::RemoteCall { module, function, args, .. }
            if module == "$terlan.managed.http" && function == &format!("session_{name}") && args.len() == arity)
        );
        assert!(matches!(
            lower_managed_http_operation(operation, |_| Ok(NativeExpr::Param(0))).unwrap(),
            Some(NativeExpr::ManagedOperation { .. })
        ));
        for wrong in [0, arity + 1] {
            *body(&mut core) = primitive(name, wrong);
            assert!(lower_http_values(&mut core)
                .unwrap_err()
                .contains("http_session_arity"));
        }
    }
}

#[test]
fn retired_session_response_operation_has_no_type_or_lowering() {
    for function in [
        "session_with_response",
        "session_response_cookie",
        "session_live_cookie",
        "session_live_identity",
    ] {
        for arity in 0..4 {
            let expression = CoreExpr::RemoteCall {
                module: "$terlan.managed.http".into(),
                function: function.into(),
                type_args: vec![],
                args: vec![CoreExpr::Int(0); arity],
            };
            assert_eq!(
                super::super::http_values::managed_http_operation_type(&expression),
                None
            );
            assert!(lower_managed_http_operation(&expression, |_| panic!(
                "retired operation must not evaluate inputs"
            ))
            .unwrap()
            .is_none());
        }
    }
}

#[test]
fn ordinary_unit_primitives_keep_their_result_and_unknown_operations_are_untouched() {
    for (name, arity) in [("set", 3), ("delete", 2), ("expire", 1)] {
        let mut core = http_core();
        core.imports.clear();
        let mut expression = primitive(name, arity);
        let CoreExpr::Intrinsic(call) = &mut expression else {
            unreachable!();
        };
        call.return_type = CoreType::Named("Unit".into());
        *body(&mut core) = expression;
        lower_http_values(&mut core).unwrap();
        assert!(
            matches!(body(&mut core), CoreExpr::RemoteCall { function, .. }
            if function == &format!("session_{name}"))
        );
    }
    for name in [
        "unknown",
        "set_extra",
        "",
        "CURRENT",
        "with_response",
        "response_cookie",
        "live_identity",
    ] {
        let mut core = http_core();
        core.imports.clear();
        let expression = primitive(name, 1);
        *body(&mut core) = expression.clone();
        assert!(http_managed_layouts(&core).unwrap().is_empty());

        lower_http_values(&mut core).unwrap();
        assert_eq!(body(&mut core), &expression);
    }
}

#[test]
fn importing_session_does_not_install_its_native_layouts() {
    let mut core = http_core();
    core.imports
        .retain(|import| import.module == "std.http.Request");
    core.imports.push(CoreImport {
        module: "std.http.Session".into(),
        kind: CoreImportKind::Module,
    });
    assert!(http_managed_layouts(&core).unwrap().is_empty());
}

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
