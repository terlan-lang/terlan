use std::collections::HashSet;

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output};

use super::{
    prune_application_to_function_roots, prune_compile_time_router_builders,
    prune_module_to_function_roots, prune_unreachable_open_std_functions, resolve_scoped_call,
    FunctionKey,
};

#[test]
fn unqualified_call_resolves_only_through_the_callers_imports() {
    let providers = vec![
        function("std.alpha.Codec", "encode_exact", 2),
        function("std.binary.Binary", "encode_exact", 2),
    ];
    let imported = HashSet::from(["std.binary.Binary"]);

    assert_eq!(
        resolve_scoped_call("app.Main", &imported, "encode_exact", 2, &providers),
        Some(function("std.binary.Binary", "encode_exact", 2))
    );
}

#[test]
fn unqualified_call_resolves_through_a_symbol_import() {
    let providers = vec![
        function("std.alpha.Codec", "encode_exact", 2),
        function("std.binary.Binary", "encode_exact", 2),
    ];
    let imported = HashSet::from(["std.binary.Binary.encode_exact"]);

    assert_eq!(
        resolve_scoped_call("app.Main", &imported, "encode_exact", 2, &providers),
        Some(function("std.binary.Binary", "encode_exact", 2))
    );
}

#[test]
fn qualified_call_resolves_without_an_import() {
    let providers = vec![function("std.binary.Binary", "encode_exact", 2)];

    assert_eq!(
        resolve_scoped_call(
            "app.Main",
            &HashSet::new(),
            "std.binary.Binary.encode_exact",
            2,
            &providers,
        ),
        Some(function("std.binary.Binary", "encode_exact", 2))
    );
}

#[test]
fn selected_test_roots_retain_only_their_local_function_closure() {
    let syntax = parse_module_as_syntax_output(
        r#"
module app.SampleTest.

pub helper(): Bool -> true.
pub selected(): Bool -> helper().
pub unrelated(): Bool -> false.
"#,
    )
    .expect("parse test-root pruning fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let mut core = lower_syntax_module_output_to_core(&syntax, &resolved);

    prune_module_to_function_roots(&mut core, &["selected"]);

    let retained = core
        .functions
        .iter()
        .map(|function| function.name.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(retained, HashSet::from(["helper", "selected"]));
    assert!(core.exports.iter().all(|export| export.name != "unrelated"));
}

#[test]
fn application_roots_do_not_turn_dead_public_functions_into_executable_abi() {
    let syntax = parse_module_as_syntax_output(
        r#"
module app.Main.

helper(): Unit -> Unit.
pub main(): Unit -> helper().
pub dead_dynamic(value: Dynamic): Dynamic -> value.
"#,
    )
    .expect("parse application-root pruning fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let mut cores = vec![lower_syntax_module_output_to_core(&syntax, &resolved)];

    prune_application_to_function_roots(
        &mut cores,
        &[("app.Main".to_string(), "main".to_string(), 0)],
    )
    .expect("prune application root");

    let retained = cores[0]
        .functions
        .iter()
        .map(|function| function.name.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(retained, HashSet::from(["helper", "main"]));
    assert!(cores[0]
        .exports
        .iter()
        .all(|export| export.name != "dead_dynamic"));
}

#[test]
fn selected_test_roots_retain_receiver_method_closure() {
    let syntax = parse_module_as_syntax_output(
        r#"
module app.ReceiverTest.

pub struct Presenter { prefix: String }.
pub struct User { name: String }.
pub (presenter: Presenter) present[T => {name: String}](value: T): String ->
    presenter.prefix + value.name.
pub selected(): Bool ->
    let presenter = Presenter {prefix: "User: "};
    presenter.present(User {name: "Ada"}) == "User: Ada".
pub unrelated(): Bool -> false.
"#,
    )
    .expect("parse receiver pruning fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let mut core = lower_syntax_module_output_to_core(&syntax, &resolved);

    prune_module_to_function_roots(&mut core, &["selected"]);

    let retained = core
        .functions
        .iter()
        .map(|function| function.name.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(retained, HashSet::from(["present", "selected"]));
}

#[test]
fn standalone_standard_package_exports_are_application_roots() {
    let syntax = parse_module_as_syntax_output(
        r#"
module std.sample.polars.DataFrame.

pub version(): Int -> 4.
"#,
    )
    .expect("parse standalone standard-package fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let mut cores = vec![core];

    prune_unreachable_open_std_functions(&mut cores);

    assert_eq!(cores[0].functions.len(), 1);
    assert_eq!(cores[0].functions[0].name, "version");
}

#[test]
fn direct_aot_prunes_static_router_builders_but_retains_handlers() {
    let syntax = parse_module_as_syntax_output(
        r#"
module app.Web.

import std.http.Router.
import std.http.Response.
import type std.http.Request.Request.
import type std.http.Response.Response.
import type std.http.Router.Router.

pub home(_request: Request): Response -> Response.text("home").
pub users(router: Router): Router -> Router.get(router, "/users", home).
pub router(): Router -> users(Router.new()).
"#,
    )
    .expect("parse static-router pruning fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let mut cores = vec![core];

    prune_compile_time_router_builders(&mut cores);

    let retained = cores[0]
        .functions
        .iter()
        .map(|function| function.name.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(retained, HashSet::from(["home"]));
    assert!(cores[0]
        .exports
        .iter()
        .all(|export| !matches!(export.name.as_str(), "router" | "users")));
}

fn function(module: &str, name: &str, arity: usize) -> FunctionKey {
    (module.to_string(), name.to_string(), arity)
}
