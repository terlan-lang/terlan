use super::test_support::*;
use crate::terlan_syntax::parse_module_as_syntax_output;

#[test]
fn std_interface_fixture_scope_includes_only_direct_and_transitive_dependencies() {
    let module = parse_module_as_syntax_output(
        "module std_scope.\n\nimport std.collections.Enumerable.\n\npub main(): Int -> 1.\n",
    )
    .expect("parse std scope fixture");

    let interfaces = crate::terlan_hir::checked_in_std_interfaces_for_module(&module);
    let mut modules = interfaces.keys().map(String::as_str).collect::<Vec<_>>();
    modules.sort_unstable();

    assert_eq!(
        modules,
        vec![
            "std.collections.Enumerable",
            "std.core.Option",
            "std.core.Ordering",
            "std.core.Unit"
        ]
    );
}

/// Verifies release core collection contracts typecheck on the formal path.
///
/// Inputs:
/// - Release source contracts for `std.collections.Map`, `std.collections.List`,
///   `std.collections.Set`, `std.collections.Iterator`,
///   `std.collections.Iterable`, and `std.collections.Enumerable`.
///
/// Output:
/// - Test passes when all promoted collection contracts produce no
///   diagnostics.
///
/// Transformation:
/// - Runs release contracts through formal syntax-output typechecking and
///   relies on `@compiler.intrinsic` declarations for compiler-provided
///   collection method implementations.
#[test]
#[ignore = "release-scale std collection sweep; run through make stdlib-release-contracts-check"]
fn syntax_output_accepts_release_core_collection_contracts() {
    let contracts = [
        (
            include_str!("../../../../../std/collections/Map.terl"),
            "std/collections/Map.terl",
        ),
        (
            include_str!("../../../../../std/collections/List.terl"),
            "std/collections/List.terl",
        ),
        (
            include_str!("../../../../../std/collections/Set.terl"),
            "std/collections/Set.terl",
        ),
        (
            include_str!("../../../../../std/collections/Iterator.terl"),
            "std/collections/Iterator.terl",
        ),
        (
            include_str!("../../../../../std/collections/Index.terl"),
            "std/collections/Index.terl",
        ),
        (
            include_str!("../../../../../std/collections/Iterable.terl"),
            "std/collections/Iterable.terl",
        ),
        (
            include_str!("../../../../../std/collections/Enumerable.terl"),
            "std/collections/Enumerable.terl",
        ),
    ];

    std::thread::scope(|scope| {
        let checks = contracts
            .into_iter()
            .map(|(source, path)| {
                scope.spawn(move || (path, check_syntax_output_with_std_interfaces(source, path)))
            })
            .collect::<Vec<_>>();

        for check in checks {
            let (path, diagnostics) = check.join().expect("collection contract typecheck");
            assert!(
                diagnostics.is_empty(),
                "unexpected release collection diagnostics in {path}: {diagnostics:?}"
            );
        }
    });
}

/// Verifies the release Task contract typechecks through std summaries.
///
/// Inputs:
/// - A small source module importing `std.core.Task` and composing a task
///   with receiver methods.
///
/// Output:
/// - Test passes when formal typechecking reports no diagnostics.
///
/// Transformation:
/// - Loads checked-in std summaries from the Task source anchor, resolves
///   the parsed module against them, and verifies the typed async surface is
///   usable without source-level `async`, `await`, send, or receive syntax.
#[test]
fn syntax_output_accepts_release_core_task_contract_usage() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module task_contract_usage.\n\
\n\
import std.core.Task.\n\
import type std.core.Result.Result.\n\
import type std.core.Error.Error.\n\
\n\
pub join(task: Task[Int]): Result[Int, Error] ->\n\
    task.result().\n\
\n\
pub complete(): Task[Int] ->\n\
    Task.done(1).\n\
",
        "std/core/Task.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected release Task diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies HTTP responses accept typed template fragments through summaries.
///
/// Inputs:
/// - A small source module importing `std.http.Response` and
///   `std.template.Template`.
///
/// Output:
/// - Test passes when `Response.html(page())` selects the `Template.Html`
///   overload from checked-in std summaries.
///
/// Transformation:
/// - Loads release std summaries from the response source anchor, resolves the
///   imported overload set, and typechecks template-fragment response creation
///   without backend emission.
#[test]
fn syntax_output_accepts_response_html_template_fragment_usage() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module response_template_usage.\n\
\n\
import std.http.Response.\n\
import std.template.Template.\n\
import type std.http.Response.Response.\n\
\n\
pub page(): Template.Html ->\n\
    Template.trusted(\"<main>Hello</main>\").\n\
\n\
pub home(): Response ->\n\
    Response.html(page()).\n\
",
        "std/http/Response.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected Response.html Template.Html diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies TLS helper calls preserve imported union alias return types.
///
/// Inputs:
/// - A source module importing `std.http.Tls.auto`.
/// - A function returning imported `std.http.Tls.Config` from that helper.
///
/// Output:
/// - Test passes when typechecking accepts the helper call without reporting a
///   mismatch between `Config.mode` and the `Mode` union's atom aliases.
///
/// Transformation:
/// - Loads checked-in std summaries from the TLS source anchor and exercises
///   imported function return-type comparison through expanded alias records.
#[test]
fn syntax_output_accepts_tls_helper_config_return_usage() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module tls_helper_usage.\n\
\n\
import std.http.Tls.{auto}.\n\
import type std.collections.List.\n\
import type std.http.Tls.Config.\n\
\n\
pub config(domains: List[String]): Config ->\n\
    auto(domains, \"admin@example.com\").\n\
",
        "std/http/Tls.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected TLS helper diagnostics: {:?}",
        diagnostics
    );
}
