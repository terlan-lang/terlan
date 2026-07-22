use super::test_support::{
    check_syntax_output, check_syntax_output_with_interface,
    check_syntax_output_with_std_interfaces,
};

/// Verifies an ordinary arithmetic function may be annotated `@pure`.
///
/// Inputs:
/// - A module with one marker-only `@pure` function.
///
/// Output:
/// - Test passes when typechecking reports no diagnostics.
///
/// Transformation:
/// - Exercises the semantic typechecker path for a pure body after syntax
///   output has already accepted the marker annotation.
#[test]
fn syntax_output_accepts_pure_function_body() {
    let diagnostics = check_syntax_output(
        "\
module pure_function_body_ok.\n\
\n\
@pure\n\
pub normalize(value: Int): Int ->\n\
    value * 100.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies `@pure` functions may call inferred pure local helpers.
///
/// Inputs:
/// - A module with one ordinary arithmetic helper and one marker-only `@pure`
///   function that calls it.
///
/// Output:
/// - Test passes when typechecking reports no diagnostics.
///
/// Transformation:
/// - Locks the first local purity inference pass so ordinary pure helpers do
///   not require redundant `@pure` annotations before use in pure call sites.
#[test]
fn syntax_output_accepts_pure_function_calling_inferred_pure_helper() {
    let diagnostics = check_syntax_output(
        "\
module pure_function_calls_inferred_helper.\n\
\n\
normalize_step(value: Int): Int ->\n\
    value * 100.\n\
\n\
@pure\n\
pub normalize(value: Int): Int ->\n\
    normalize_step(value).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies `@pure` functions may return inert `Effect[T]` descriptions.
///
/// Inputs:
/// - A module importing `std.core.Effect` and one marker-only `@pure` function
///   that returns `Effect.succeed(value)`.
///
/// Output:
/// - Test passes when typechecking reports no diagnostics.
///
/// Transformation:
/// - Locks the roadmap contract that constructing an effect description is
///   pure; executing the effect remains a separate VM/runtime boundary.
#[test]
fn syntax_output_accepts_pure_function_returning_effect_description() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module pure_function_returns_effect.\n\
\n\
import std.core.Effect.{Effect, succeed}.\n\
\n\
@pure\n\
pub plan(value: Int): Effect[Int] ->\n\
    succeed(value).\n\
",
        "std/core/Effect.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies executing an Effect is rejected from an asserted pure function.
///
/// Inputs:
/// - A pure function that constructs and executes one completed Effect.
///
/// Output:
/// - A stable imported-effect purity diagnostic.
///
/// Transformation:
/// - Proves the compiler distinguishes inert Effect construction from the
///   VM-owned execution boundary.
#[test]
fn syntax_output_rejects_effect_execution_inside_pure_function() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module pure_function_executes_effect.\n\
\n\
import std.core.Effect.{Effect, run, succeed}.\n\
\n\
@pure\n\
pub execute(value: Int): Int ->\n\
    run(succeed(value)).\n\
",
        "std/core/Effect.terl",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains(
            "function execute annotated @pure must be pure; found effectful imported function call"
        )
        }),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies `@pure` functions reject structurally effectful bodies.
///
/// Inputs:
/// - A marker-only `@pure` function whose body performs indexed assignment.
///
/// Output:
/// - Test passes when typechecking reports a stable purity diagnostic.
///
/// Transformation:
/// - Locks `@pure` to a checked body invariant rather than a trusted source
///   promise.
#[test]
fn syntax_output_rejects_impure_pure_function_body() {
    let diagnostics = check_syntax_output(
        "\
module pure_function_body_impure.\n\
\n\
@pure\n\
pub replace_at(items: List[Int]): Unit ->\n\
    items[0] = 1.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag.message.contains(
            "function replace_at annotated @pure must be pure; found indexed assignment"
        )),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies `@pure` receiver methods reject structurally effectful bodies.
///
/// Inputs:
/// - An opaque receiver type and one marker-only `@pure` receiver method whose
///   body performs indexed assignment on an argument.
///
/// Output:
/// - Test passes when typechecking reports a stable purity diagnostic.
///
/// Transformation:
/// - Keeps method-level purity metadata semantically checked through the same
///   callable body path as ordinary functions.
#[test]
fn syntax_output_rejects_impure_pure_receiver_method_body() {
    let diagnostics = check_syntax_output(
        "\
module pure_receiver_body_impure.\n\
\n\
opaque type Box[T].\n\
\n\
@pure\n\
(box: Box[Int]) replace_at(items: List[Int]): Unit ->\n\
    items[0] = 1.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag.message.contains(
            "receiver method replace_at annotated @pure must be pure; found indexed assignment"
        )),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies `@pure` functions reject raw SQL macro bodies.
///
/// Inputs:
/// - A row struct and a marker-only `@pure` function whose body is a typed SQL
///   raw macro.
///
/// Output:
/// - Test passes when typechecking reports the stable purity diagnostic for a
///   raw macro expression.
///
/// Transformation:
/// - Keeps database query forms out of pure function bodies even before full
///   SQL effect execution is wired into the VM.
#[test]
fn syntax_output_rejects_raw_macro_in_pure_function_body() {
    let diagnostics = check_syntax_output(
        "\
module pure_function_body_raw_macro.\n\
\n\
pub struct UserRow {\n\
    id: Int\n\
}.\n\
\n\
@pure\n\
pub load_user(): Dynamic ->\n\
    sql[UserRow] {select id from users}.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("function load_user annotated @pure must be pure; found raw macro")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies `@pure` functions reject HTML rendering bodies.
///
/// Inputs:
/// - A marker-only `@pure` function whose body is an `html { ... }` block.
///
/// Output:
/// - Test passes when typechecking reports the stable purity diagnostic for an
///   HTML block.
///
/// Transformation:
/// - Keeps rendering/runtime work outside pure helpers until template purity
///   can be proven with richer compiler metadata.
#[test]
fn syntax_output_rejects_html_block_in_pure_function_body() {
    let diagnostics = check_syntax_output(
        "\
module pure_function_body_html.\n\
\n\
@pure\n\
pub view(title: Binary): Dynamic ->\n\
    html {\n\
        <section>{title}</section>\n\
    }.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("function view annotated @pure must be pure; found html block")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies `@pure` functions reject template instantiation bodies.
///
/// Inputs:
/// - A declared template and a marker-only `@pure` function whose body calls
///   the generated template function.
///
/// Output:
/// - Test passes when typechecking reports the stable purity diagnostic for
///   template instantiation.
///
/// Transformation:
/// - Exercises the same normalized template-call path used by real programs so
///   purity validation cannot accidentally allow rendering through `Page(...)`.
#[test]
fn syntax_output_rejects_template_instantiation_in_pure_function_body() {
    let diagnostics = check_syntax_output(
        "\
module pure_function_body_template_instantiation.\n\
\n\
template Page from \"./templates/page.terl.html\" {\n\
    title: Binary\n\
}.\n\
\n\
@pure\n\
pub view(title: Binary): Html[Dynamic] ->\n\
    Page(title = title).\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("function view annotated @pure must be pure; found template instantiation")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies `@pure` functions reject OS entropy acquisition.
///
/// Inputs:
/// - A module importing `std.random.Random`.
/// - A marker-only `@pure` function whose body calls `Random.entropy()`.
///
/// Output:
/// - Test passes when typechecking reports a stable purity diagnostic for an
///   effectful imported call.
///
/// Transformation:
/// - Keeps NativeBoundary-backed randomness outside pure helpers because it
///   observes external entropy instead of constructing an inert value.
#[test]
fn syntax_output_rejects_entropy_call_in_pure_function_body() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module pure_function_body_entropy.\n\
\n\
import std.random.Random.\n\
import type std.random.Random.{Generator}.\n\
\n\
@pure\n\
pub generator(): Generator ->\n\
    Random.entropy().\n\
",
        "std/random/Random.terl",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("function generator annotated @pure must be pure; found effectful imported function call")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies `@pure` functions reject console output.
///
/// Inputs:
/// - A module importing `std.io.Console`.
/// - A marker-only `@pure` function whose body calls `Console.println(...)`.
///
/// Output:
/// - Test passes when typechecking reports a stable purity diagnostic for an
///   effectful imported call.
///
/// Transformation:
/// - Extends the imported std-effect taxonomy beyond randomness so ordinary
///   IO cannot be hidden behind compiler intrinsic facade calls.
#[test]
fn syntax_output_rejects_console_println_call_in_pure_function_body() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module pure_function_body_console_println.\n\
\n\
import std.io.Console.\n\
\n\
@pure\n\
pub print(): Unit ->\n\
    Console.println(\"hello\").\n\
",
        "std/io/Console.terl",
    );

    assert!(
        diagnostics.iter().any(|diag| diag.message.contains(
            "function print annotated @pure must be pure; found effectful imported function call"
        )),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies `@pure` functions reject filesystem reads.
///
/// Inputs:
/// - A module importing `std.io.File`.
/// - A marker-only `@pure` function whose body calls `File.read_text(...)`.
///
/// Output:
/// - Test passes when typechecking reports a stable purity diagnostic for an
///   effectful imported call.
///
/// Transformation:
/// - Locks filesystem observation as an imported std effect, preserving pure
///   helper invariants even when the call is routed through a portable facade.
#[test]
fn syntax_output_rejects_file_read_call_in_pure_function_body() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module pure_function_body_file_read.\n\
\n\
import std.io.File.\n\
import type std.core.Result.\n\
import type std.io.File.{FileError}.\n\
\n\
@pure\n\
pub read(): Result[String, FileError] ->\n\
    File.read_text(\"/tmp/terlan.txt\").\n\
",
        "std/io/File.terl",
    );

    assert!(
        diagnostics.iter().any(|diag| diag.message.contains(
            "function read annotated @pure must be pure; found effectful imported function call"
        )),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies `@pure` functions reject filesystem observation and mutation.
///
/// Inputs:
/// - A module importing `std.io.File`.
/// - Marker-only `@pure` functions calling the remaining portable file
///   operations.
///
/// Output:
/// - Test passes when each operation reports the stable purity diagnostic for
///   an effectful imported call.
///
/// Transformation:
/// - Keeps the std.io File taxonomy complete for observation and mutation
///   helpers, not only text reads.
#[test]
fn syntax_output_rejects_file_effect_calls_in_pure_function_body() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module pure_function_body_file_effects.\n\
\n\
import std.io.File.\n\
\n\
@pure\n\
pub exists(): Bool ->\n\
    File.exists(\"/tmp/terlan.txt\").\n\
\n\
@pure\n\
pub write(): Dynamic ->\n\
    File.write_text(\"/tmp/terlan.txt\", \"hello\").\n\
\n\
@pure\n\
pub append(): Dynamic ->\n\
    File.append_text(\"/tmp/terlan.txt\", \"hello\").\n\
\n\
@pure\n\
pub delete(): Dynamic ->\n\
    File.delete(\"/tmp/terlan.txt\").\n\
",
        "std/io/File.terl",
    );

    for function_name in ["exists", "write", "append", "delete"] {
        let expected = format!(
            "function {function_name} annotated @pure must be pure; found effectful imported function call"
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message.contains(&expected)),
            "missing diagnostic `{expected}` in {:?}",
            diagnostics
        );
    }
}

/// Verifies `@pure` functions reject calls to effectful local helpers.
///
/// Inputs:
/// - A module with one ordinary helper whose body performs indexed assignment.
/// - A marker-only `@pure` function that calls the helper.
///
/// Output:
/// - Test passes when typechecking reports a stable transitive purity
///   diagnostic.
///
/// Transformation:
/// - Proves `@pure` validation is not limited to direct syntax in the annotated
///   body; same-module helper calls cannot hide already-known effects.
#[test]
fn syntax_output_rejects_pure_function_calling_effectful_local_helper() {
    let diagnostics = check_syntax_output(
        "\
module pure_function_calls_effectful_helper.\n\
\n\
replace_at(items: List[Int]): Unit ->\n\
    items[0] = 1.\n\
\n\
@pure\n\
pub normalize(items: List[Int]): Unit ->\n\
    replace_at(items).\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag.message.contains(
            "function normalize annotated @pure must be pure; found effectful local function call"
        )),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies `@pure` functions reject transitive calls to effectful local helpers.
///
/// Inputs:
/// - A module where `replace_at` performs indexed assignment.
/// - A middle helper that calls `replace_at`.
/// - A marker-only `@pure` function that calls the middle helper.
///
/// Output:
/// - Test passes when typechecking reports the stable local-call purity diagnostic.
///
/// Transformation:
/// - Locks same-module purity inference as a fixed-point over local calls instead
///   of a one-hop direct-effect scan.
#[test]
fn syntax_output_rejects_pure_function_calling_transitively_effectful_local_helper() {
    let diagnostics = check_syntax_output(
        "\
module pure_function_calls_transitive_effectful_helper.\n\
\n\
replace_at(items: List[Int]): Unit ->\n\
    items[0] = 1.\n\
\n\
normalize_step(items: List[Int]): Unit ->\n\
    replace_at(items).\n\
\n\
@pure\n\
pub normalize(items: List[Int]): Unit ->\n\
    normalize_step(items).\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag.message.contains(
            "function normalize annotated @pure must be pure; found effectful local function call"
        )),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies local purity fixed-point analysis is independent of declaration order.
///
/// Inputs:
/// - A middle helper declared before the effectful helper it calls.
/// - A marker-only `@pure` function that calls the middle helper.
///
/// Output:
/// - Test passes when typechecking reports the stable local-call purity
///   diagnostic.
///
/// Transformation:
/// - Prevents the same-module purity pre-pass from becoming a single forward
///   scan that misses effects hidden behind later declarations.
#[test]
fn syntax_output_rejects_pure_function_calling_late_transitively_effectful_local_helper() {
    let diagnostics = check_syntax_output(
        "\
module pure_function_calls_late_transitive_effectful_helper.\n\
\n\
normalize_step(items: List[Int]): Unit ->\n\
    replace_at(items).\n\
\n\
@pure\n\
pub normalize(items: List[Int]): Unit ->\n\
    normalize_step(items).\n\
\n\
replace_at(items: List[Int]): Unit ->\n\
    items[0] = 1.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag.message.contains(
            "function normalize annotated @pure must be pure; found effectful local function call"
        )),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies guard effects propagate through a recursive local call cycle.
///
/// Inputs:
/// - Two mutually recursive helpers where one clause guard mutates a value.
/// - A marker-only `@pure` function that enters the cycle through the helper
///   without the direct guard effect.
///
/// Output:
/// - Test passes when the pure caller reports the stable effectful-local-call
///   diagnostic.
///
/// Transformation:
/// - Proves fixed-point inference scans guards as well as bodies and propagates
///   the effect through a strongly connected local call component.
#[test]
fn syntax_output_rejects_pure_function_calling_guard_effectful_recursive_cycle() {
    let diagnostics = check_syntax_output(
        "\
module pure_function_calls_guard_effectful_recursive_cycle.\n\
\n\
cycle_a(items) where items[0] = 1 -> cycle_b(items);\n\
cycle_a(items) -> cycle_b(items).\n\
\n\
cycle_b(items: List[Int]): Unit ->\n\
    cycle_a(items).\n\
\n\
@pure\n\
pub normalize(items: List[Int]): Unit ->\n\
    cycle_b(items).\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag.message.contains(
            "function normalize annotated @pure must be pure; found effectful local function call"
        )),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies recursive calls are not conservatively classified as effects.
///
/// Inputs:
/// - Two mutually recursive Bool helpers with no direct effects.
/// - A marker-only `@pure` function that enters the recursive component.
///
/// Output:
/// - Test passes when typechecking reports no diagnostics.
///
/// Transformation:
/// - Locks the least fixed-point behavior: recursion alone does not taint a
///   component when no member performs an effect.
#[test]
fn syntax_output_accepts_pure_function_calling_pure_recursive_cycle() {
    let diagnostics = check_syntax_output(
        "\
module pure_function_calls_pure_recursive_cycle.\n\
\n\
cycle_a(value: Int): Bool ->\n\
    cycle_b(value).\n\
\n\
cycle_b(value: Int): Bool ->\n\
    cycle_a(value).\n\
\n\
@pure\n\
pub classify(value: Int): Bool ->\n\
    cycle_a(value).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies selected imported calls may carry an explicit purity proof.
#[test]
fn syntax_output_accepts_pure_function_calling_pure_import() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module pure_function_calls_pure_import.\n\
\n\
import provider.Math.{normalize}.\n\
\n\
@pure\n\
pub run(value: Int): Int ->\n\
    normalize(value).\n\
",
        "\
module provider.Math.\n\
\n\
@pure\n\
pub normalize(value: Int): Int.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies selected imported calls require an explicit purity proof.
#[test]
fn syntax_output_rejects_pure_function_calling_impure_import() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module pure_function_calls_impure_import.\n\
\n\
import provider.External.{load}.\n\
\n\
@pure\n\
pub run(value: Int): Int ->\n\
    load(value).\n\
",
        "\
module provider.External.\n\
\n\
pub load(value: Int): Int.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag.message.contains(
            "function run annotated @pure must be pure; found effectful imported function call"
        )),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies imported effects propagate through inferred local helpers.
#[test]
fn syntax_output_rejects_pure_function_calling_transitively_impure_import() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module pure_function_calls_transitively_impure_import.\n\
\n\
import provider.External.{load}.\n\
\n\
load_step(value: Int): Int ->\n\
    load(value).\n\
\n\
@pure\n\
pub run(value: Int): Int ->\n\
    load_step(value).\n\
",
        "\
module provider.External.\n\
\n\
pub load(value: Int): Int.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag.message.contains(
            "function run annotated @pure must be pure; found effectful local function call"
        )),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies qualified imported calls may carry an explicit purity proof.
#[test]
fn syntax_output_accepts_pure_function_calling_pure_qualified_import() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module pure_function_calls_pure_qualified_import.\n\
\n\
import provider.Math.\n\
\n\
@pure\n\
pub run(value: Int): Int ->\n\
    Math.normalize(value).\n\
",
        "\
module provider.Math.\n\
\n\
@pure\n\
pub normalize(value: Int): Int.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies qualified imported calls require an explicit purity proof.
#[test]
fn syntax_output_rejects_pure_function_calling_impure_qualified_import() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module pure_function_calls_impure_qualified_import.\n\
\n\
import provider.External.\n\
\n\
@pure\n\
pub run(value: Int): Int ->\n\
    External.load(value).\n\
",
        "\
module provider.External.\n\
\n\
pub load(value: Int): Int.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag.message.contains(
            "function run annotated @pure must be pure; found effectful imported function call"
        )),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies qualified imported effects propagate through inferred helpers.
#[test]
fn syntax_output_rejects_pure_function_calling_transitively_impure_qualified_import() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module pure_function_calls_transitively_impure_qualified_import.\n\
\n\
import provider.External.\n\
\n\
load_step(value: Int): Int ->\n\
    External.load(value).\n\
\n\
@pure\n\
pub run(value: Int): Int ->\n\
    load_step(value).\n\
",
        "\
module provider.External.\n\
\n\
pub load(value: Int): Int.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag.message.contains(
            "function run annotated @pure must be pure; found effectful local function call"
        )),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies aliased module calls preserve explicit imported purity proofs.
#[test]
fn syntax_output_accepts_pure_function_calling_pure_module_alias() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module pure_function_calls_pure_module_alias.\n\
\n\
import provider.{Math as Numeric}.\n\
\n\
@pure\n\
pub run(value: Int): Int ->\n\
    Numeric.normalize(value).\n\
",
        "\
module provider.Math.\n\
\n\
@pure\n\
pub normalize(value: Int): Int.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies aliased module calls cannot hide an unproven imported effect.
#[test]
fn syntax_output_rejects_pure_function_calling_impure_module_alias() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module pure_function_calls_impure_module_alias.\n\
\n\
import provider.{External as Service}.\n\
\n\
@pure\n\
pub run(value: Int): Int ->\n\
    Service.load(value).\n\
",
        "\
module provider.External.\n\
\n\
pub load(value: Int): Int.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag.message.contains(
            "function run annotated @pure must be pure; found effectful imported function call"
        )),
        "diagnostics: {:?}",
        diagnostics
    );
}
