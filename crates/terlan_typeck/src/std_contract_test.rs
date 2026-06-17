use super::test_support::*;

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
fn syntax_output_accepts_release_core_collection_contracts() {
    let contracts = [
        include_str!("../../../std/collections/map.terl"),
        include_str!("../../../std/collections/list.terl"),
        include_str!("../../../std/collections/set.terl"),
        include_str!("../../../std/collections/iterator.terl"),
        include_str!("../../../std/collections/index.terl"),
        include_str!("../../../std/collections/iterable.terl"),
        include_str!("../../../std/collections/enumerable.terl"),
    ];

    for (source, std_relative_path) in contracts.into_iter().zip([
        "std/collections/map.terl",
        "std/collections/list.terl",
        "std/collections/set.terl",
        "std/collections/iterator.terl",
        "std/collections/index.terl",
        "std/collections/iterable.terl",
        "std/collections/enumerable.terl",
    ]) {
        let diagnostics = check_syntax_output_with_std_interfaces(source, std_relative_path);

        assert!(
            diagnostics.is_empty(),
            "unexpected release collection diagnostics in {:?}",
            diagnostics
        );
    }
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
        "std/core/task.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected release Task diagnostics: {:?}",
        diagnostics
    );
}
