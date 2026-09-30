//! Mutation names must not override ordinary provider methods or local receivers.

use super::check_sources;
use crate::compiler::native_ir::{
    http_values::lower_http_values, source_constructor_test::checked_provider,
};
use crate::terlan_typeck::{CoreEffectSet, CoreExpr, CoreIntrinsicCall, CoreIntrinsicId, CoreType};

#[test]
fn response_mutators_execute_provider_bodies_and_preserve_command_results() {
    let provider = r#"module std.http.Response.
pub struct Response { code: Int, label: String }.
pub start(): Response -> Response { code: 10, label: "source" }.
pub (mut response: Response) status(code: Int): Unit ->
    Response { code: response.code + code, label: response.label }.
pub (mut response: Response) header(name: String, value: String): Unit ->
    Response { code: response.code, label: response.label + ":" + name + ":" + value }.
pub (mut response: Response) set_cookie_header(value: String): Unit ->
    Response { code: response.code, label: response.label + ":cookie:" + value }.
"#;
    let caller = r#"module response_mutation_authority.
import std.http.Response.
import std.core.Unit.
pub struct Other { count: Int }.
pub (mut value: Other) status(code: Int): Unit -> Other { count: value.count + code }.
pub check(): Bool ->
    let response = Response.start();
    let snapshot = response;
    let changed = response.status(7);
    let appended = response.header("name", "value");
    let cookie = response.set_cookie_header("header");
    let other = Other { count: 2 };
    let other_done = other.status(3);
    changed == Unit and appended == Unit and cookie == Unit and other_done == Unit
        and response.code == 17 and response.label == "source:name:value:cookie:header"
        and snapshot.code == 10 and snapshot.label == "source" and other.count == 5.
"#;
    check_sources(&[caller, provider]);
    check_sources(&[
        &caller.replace("std.http.Response", "app.Response"),
        &provider.replace("std.http.Response", "app.Response"),
    ]);
}

#[test]
fn response_receiver_names_and_retired_cookie_native_operation_are_not_substituted() {
    let mut core =
        checked_provider("module caller. import std.http.Response. pub check(): Bool -> true.");
    for method in ["status", "header", "set_cookie_header"] {
        let args = vec![CoreExpr::Int(1); if method == "header" { 2 } else { 1 }];
        let mutable = CoreExpr::MutableReceiverCall {
            receiver: Box::new(CoreExpr::Var("unrelated".into())),
            method: method.into(),
            args: args.clone(),
            effects: CoreEffectSet {
                effects: vec!["state".into()],
            },
        };
        let remote = CoreExpr::RemoteCall {
            module: "__receiver__".into(),
            function: method.into(),
            type_args: vec![],
            args: std::iter::once(CoreExpr::Var("unrelated".into()))
                .chain(args)
                .collect(),
        };
        for expression in [mutable, remote] {
            core.functions[0].clauses[0].body.core_expr = Some(expression.clone());
            lower_http_values(&mut core).unwrap();
            assert_eq!(
                core.functions[0].clauses[0].body.core_expr,
                Some(expression)
            );
        }
    }
    let retired = primitive("set_cookie_header", 2);
    core.functions[0].clauses[0].body.core_expr = Some(retired.clone());
    lower_http_values(&mut core).unwrap();
    assert_eq!(core.functions[0].clauses[0].body.core_expr, Some(retired));
}

#[test]
fn retired_response_mutations_never_gain_compiler_semantics() {
    let mut core = checked_provider("module std.http.Response. pub check(): Bool -> true.");
    for (name, arity) in [("status", 2), ("header", 3)] {
        for wrong in [0, 1, arity + 1] {
            let expression = primitive(name, wrong);
            core.functions[0].clauses[0].body.core_expr = Some(expression.clone());
            lower_http_values(&mut core).unwrap();
            assert_eq!(
                core.functions[0].clauses[0].body.core_expr,
                Some(expression)
            );
        }
    }
}

fn primitive(name: &str, arity: usize) -> CoreExpr {
    CoreExpr::Intrinsic(CoreIntrinsicCall {
        id: CoreIntrinsicId::NativeOperation {
            operation: format!("std.http.response.{name}"),
            parameter_types: vec![],
        },
        args: vec![CoreExpr::Int(0); arity],
        return_type: CoreType::Named("Response".into()),
        effects: CoreEffectSet { effects: vec![] },
        span: crate::terlan_syntax::span::Span::new(0, 0),
    })
}
