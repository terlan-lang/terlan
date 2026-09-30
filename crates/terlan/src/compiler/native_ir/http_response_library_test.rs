//! Response composition and policy are source functions, not compiler substitutes.

use super::source_constructor_test::check_sources;

#[path = "http_response_mutation_test.rs"]
mod mutation_authority_test;

#[test]
fn response_storage_and_updates_execute_standard_source() {
    let provider = format!(
        "{}\n{}",
        include_str!("../../../../../std/http/Response.terl"),
        r#"
pub check(): Bool ->
    let original = text("body", 200);
    let changed = original.with_status(218).with_header("X-Test", "first").with_header("X-Test", "second");
    let moved = redirect("/next", 302);
    let streamed = stream(["a", "b"], 202, "application/custom", 7, 2);
    original.#status == 200 and original.#headers == []
        and changed.#status == 218 and changed.#payload == "body"
        and changed.#headers == [{"X-Test", "first"}, {"X-Test", "second"}]
        and moved.#payload == "" and moved.#status == 302
        and moved.#headers == [{"Location", "/next"}]
        and streamed.#kind == 5 and streamed.#chunks == ["a", "b"]
        and streamed.#chunk_size == 7 and streamed.#max_pending_writes == 2
        and streamed.#content_type == "application/custom".
"#
    );
    check_sources(&[&provider]);
    check_sources(&[&provider.replace("module std.http.Response.", "module app.SourceResponse.")]);
}

#[test]
fn response_storage_remains_private_to_the_source_module() {
    use crate::terlan_hir::{
        resolve_syntax_module_output_with_interfaces, syntax_module_output_to_interface,
    };
    use crate::terlan_syntax::parse_module_as_syntax_output;
    use crate::terlan_typeck::type_check_syntax_module_output;
    let provider =
        parse_module_as_syntax_output(include_str!("../../../../../std/http/Response.terl"))
            .unwrap();
    for (body, result) in [
        ("value.#payload", "String"),
        ("value.#status", "Int"),
        ("value.payload", "String"),
    ] {
        let syntax = parse_module_as_syntax_output(&format!("module app.HiddenResponse. import std.http.Response. pub inspect(value: Response.Response): {result} -> {body}.")).unwrap();
        let interfaces = std::collections::HashMap::from([(
            "std.http.Response".into(),
            syntax_module_output_to_interface(&provider),
        )]);
        let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
        assert!(
            resolved.diagnostics.is_empty(),
            "{:?}",
            resolved.diagnostics
        );
        let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
        assert!(
            !diagnostics.is_empty(),
            "private storage leaked through {body}"
        );
    }
}

#[test]
fn security_policy_rejects_invalid_fields_through_ordinary_type_checking() {
    use crate::terlan_hir::{
        checked_in_std_interfaces_for_module, resolve_syntax_module_output_with_interfaces,
    };
    use crate::terlan_syntax::parse_module_as_syntax_output;
    use crate::terlan_typeck::type_check_syntax_module_output;

    for arguments in [
        "true, 42, NoReferrer, 0, false",
        "true, Deny, 42, 0, false",
        "0, Deny, NoReferrer, 0, false",
        "true, Deny, NoReferrer, false, false",
        "true, Deny, NoReferrer, 0, 1",
    ] {
        let source = format!(
            "module invalid_policy. import std.http.Response. import std.http.Response.{{SecurityHeaders, Deny, NoReferrer}}. pub policy(): SecurityHeaders -> SecurityHeaders({arguments})."
        );
        let syntax = parse_module_as_syntax_output(&source).unwrap();
        let interfaces = checked_in_std_interfaces_for_module(&syntax);
        let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces);
        assert!(
            resolved.module.diagnostics.is_empty(),
            "{:?}",
            resolved.module.diagnostics
        );
        let diagnostics = type_check_syntax_module_output(&syntax, &resolved.module);
        assert!(
            !diagnostics.is_empty(),
            "invalid policy accepted: {arguments}"
        );
    }
}

#[test]
fn security_header_application_executes_provider_methods() {
    let provider = r#"
module std.http.Response.
pub struct Response { label: String }.
pub struct SecurityHeaders { label: String }.
pub start(): Response -> Response { label: "start" }.
pub policy(): SecurityHeaders -> SecurityHeaders { label: "source" }.
pub (mut response: Response) security_headers(policy: SecurityHeaders): Response ->
    Response { label: response.label + ":" + policy.label }.
pub (mut response: Response) with_security_headers(policy: SecurityHeaders): Response ->
    response |> security_headers(policy).
"#;
    let caller = r#"
module security_application_authority.
import std.http.Response.
import std.http.Response.{security_headers as apply_policy}.
pub check(): Bool ->
    let policy = Response.policy();
    let first = Response.start().with_security_headers(policy);
    let second = Response.security_headers(first, policy);
    let third = apply_policy(second, policy);
    first.label == "start:source" and second.label == "start:source:source"
        and third.label == "start:source:source:source".
"#;
    check_sources(&[caller, provider]);
    check_sources(&[
        &caller.replace("std.http.Response", "app.Response"),
        &provider.replace("std.http.Response", "app.Response"),
    ]);
}

#[test]
fn security_named_constructors_are_not_replaced_by_http_layouts() {
    use crate::terlan_typeck::CoreExpr;
    let mut core = super::source_constructor_test::checked_provider(
        "module security_namesake. import std.http.Response. pub check(): Bool -> true.",
    );
    for owner in ["app.Policy", "std.http.Response"] {
        let expression = CoreExpr::ConstructorCall {
            constructor: format!("{owner}.SecurityHeaders"),
            constructor_identity: Some(format!("{owner}.SecurityHeaders")),
            type_args: Vec::new(),
            args: (0..5).map(CoreExpr::Int).collect(),
        };
        core.functions[0].clauses[0].body.core_expr = Some(expression.clone());
        super::http_values::lower_http_values(&mut core).unwrap();
        assert_eq!(
            core.functions[0].clauses[0].body.core_expr.as_ref(),
            Some(&expression)
        );
    }
}

#[test]
fn standard_response_has_no_native_operations_or_compiler_owned_layouts() {
    let mut core = super::source_constructor_test::checked_provider(include_str!(
        "../../../../../std/http/Response.terl"
    ));
    assert!(core
        .functions
        .iter()
        .all(|function| function.native_operation.is_none()));
    assert!(super::http_values::http_managed_layouts(&core)
        .unwrap()
        .is_empty());
    super::http_values::lower_http_values(&mut core).unwrap();
    assert!(super::http_values::http_managed_layouts(&core)
        .unwrap()
        .is_empty());
}

#[test]
fn response_composition_executes_source_receiver_methods() {
    let provider = r#"
module std.http.Response.
pub struct Response { code: Int, label: String }.
pub from_code(code: Int): Response -> Response { code: code, label: "start" }.
pub (mut response: Response) with_status(code: Int): Response ->
    Response { code: code + response.code, label: response.label }.
pub (mut response: Response) with_header(name: String, value: String): Response ->
    Response { code: response.code, label: name + ":" + value }.
"#;
    let caller = r#"
module response_composition_authority.
import std.http.Response.
import std.http.Response.{with_header as named_header}.
pub check(): Bool ->
    let response = Response.from_code(2).with_status(40).with_header("key", "value");
    let qualified = Response.with_status(Response.from_code(3), 7);
    let aliased = named_header(qualified, "alias", "body");
    response.code == 42 and response.label == "key:value"
        and qualified.code == 10 and aliased.label == "alias:body".
"#;
    check_sources(&[caller, provider]);
    check_sources(&[
        &caller.replace("std.http.Response", "app.Response"),
        &provider.replace("std.http.Response", "app.Response"),
    ]);
}

#[test]
fn ordinary_response_calls_retain_explicit_types_and_namespace_boundaries() {
    use crate::terlan_typeck::{CoreExpr, CoreType};
    let mut core = super::source_constructor_test::checked_provider(
        "module response_calls. import std.http.Response. pub check(): Bool -> true.",
    );
    let remote = CoreExpr::RemoteCall {
        module: "std.http.Response".into(),
        function: "source_policy".into(),
        type_args: vec![CoreType::String],
        args: vec![CoreExpr::Binary("\"value\"".into())],
    };
    let qualified = CoreExpr::Call {
        function: "std.http.Response.source_policy".into(),
        type_args: vec![CoreType::String],
        args: vec![CoreExpr::Binary("\"value\"".into())],
    };
    let namesake = CoreExpr::Call {
        function: "std.http.ResponsePolicy.default_security_headers".into(),
        type_args: Vec::new(),
        args: Vec::new(),
    };
    for (input, expected) in [
        (remote.clone(), remote.clone()),
        (qualified.clone(), qualified),
        (namesake.clone(), namesake),
    ] {
        core.functions[0].clauses[0].body.core_expr = Some(input);
        super::http_values::lower_http_values(&mut core).unwrap();
        assert_eq!(
            core.functions[0].clauses[0].body.core_expr.as_ref(),
            Some(&expected)
        );
    }
}

#[test]
fn security_policy_defaults_follow_provider_bodies_and_import_aliases() {
    let provider = r#"
module std.http.Response.
pub default_security_headers(): Int -> 17.
pub production_security_headers(): Int -> default_security_headers() + 25.
"#;
    let caller = r#"
module response_policy_authority.
import std.http.Response.
import std.http.Response.{default_security_headers as defaults, production_security_headers as production}.
default_security_headers(): Int -> 103.
pub check(): Bool ->
    Response.default_security_headers() == 17 and defaults() == 17
        and Response.production_security_headers() == 42 and production() == 42
        and default_security_headers() == 103.
"#;
    check_sources(&[caller, provider]);
    check_sources(&[
        &caller.replace("std.http.Response", "app.Response"),
        &provider.replace("std.http.Response", "app.Response"),
    ]);
}

#[test]
fn standard_security_policy_defaults_execute_as_source_records() {
    check_sources(&[
        r#"
module response_policy_values.
import std.http.Response.
import std.http.Response.{Deny, StrictOriginWhenCrossOrigin}.
pub check(): Bool ->
    let local = Response.default_security_headers();
    let deployed = Response.production_security_headers();
    local.content_type_options and local.frame_options == Deny
        and local.referrer_policy == StrictOriginWhenCrossOrigin
        and local.hsts_max_age == 0 and not local.hsts_include_subdomains
        and deployed.content_type_options and deployed.frame_options == Deny
        and deployed.referrer_policy == StrictOriginWhenCrossOrigin
        and deployed.hsts_max_age == 31536000 and deployed.hsts_include_subdomains.
"#,
        include_str!("../../../../../std/http/Response.terl"),
    ]);
}
