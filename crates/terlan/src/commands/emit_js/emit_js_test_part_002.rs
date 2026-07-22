
/// Verifies that constructor calls stay outside the direct JS backend
/// subset until their runtime representation is selected.
///
/// Inputs:
/// - A checked Terlan module with a declared constructor and a public
///   function returning a constructor call.
///
/// Output:
/// - Assertions over direct-AST rejection and fallback JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies the direct Oxc
///   AST emitter declines `CoreExpr::ConstructorCall`, then checks the
///   command-facing facade preserves the JS stub fallback.
#[test]
fn emit_core_module_with_oxc_codegen_falls_back_for_constructor_call() {
    let source = "\
module js_core_constructor_call_fallback.

pub constructor Ok {
    (value: Int): Dynamic -> value
}.

pub make(): Dynamic ->
    Ok(1).
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_constructor_call_fallback.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    assert!(oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core).is_none());

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("fallback Oxc backend emits bootstrap JS");

    assert!(js.contains("export function make()"));
    assert!(
        js.contains("throw new Error(\"Terlan JS backend stub\")"),
        "{js}"
    );
}

/// Verifies that constructor chains stay outside the direct JS backend
/// subset until their runtime representation is selected.
///
/// Inputs:
/// - A checked Terlan module with a declared constructor and a public
///   function returning a constructor-chain expression.
///
/// Output:
/// - Assertions over direct-AST rejection and fallback JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies the direct Oxc
///   AST emitter declines `CoreExpr::ConstructorChain`, then checks the
///   command-facing facade preserves the JS stub fallback.
#[test]
fn emit_core_module_with_oxc_codegen_falls_back_for_constructor_chain() {
    let source = "\
module js_core_constructor_chain_fallback.

pub constructor User {
    (id: Int, name: Binary): Dynamic -> id
}.

pub make(id: Int, name: Binary): Dynamic ->
    User(id, name) with Admin { id: id, name: name }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_constructor_chain_fallback.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    assert!(oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core).is_none());

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("fallback Oxc backend emits bootstrap JS");

    assert!(js.contains("export function make(id, name)"));
    assert!(
        js.contains("throw new Error(\"Terlan JS backend stub\")"),
        "{js}"
    );
}

/// Verifies that try expressions stay outside the direct JS backend subset
/// until exception and cleanup semantics are selected.
///
/// Inputs:
/// - A checked Terlan module with a public function returning a `try`
///   expression with `of`, `catch`, and `after` clauses.
///
/// Output:
/// - Assertions over direct-AST rejection and fallback JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies the direct Oxc
///   AST emitter declines `CoreExpr::Try`, then checks the command-facing
///   facade preserves the JS stub fallback.
#[test]
fn emit_core_module_with_oxc_codegen_falls_back_for_try_expr() {
    let source = "\
module js_core_try_fallback.

pub run(): Dynamic ->
    try 1 {
        value -> value
    catch
        reason -> reason
    after
        0 -> Atom[\"done\"]
    }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_try_fallback.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    assert!(oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core).is_none());

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("fallback Oxc backend emits bootstrap JS");

    assert!(js.contains("export function run()"));
    assert!(
        js.contains("throw new Error(\"Terlan JS backend stub\")"),
        "{js}"
    );
}

/// Verifies that quote expressions stay outside the direct JS backend
/// subset until macro-AST runtime semantics are selected.
///
/// Inputs:
/// - A checked Terlan module with a public function returning `quote 1`.
///
/// Output:
/// - Assertions over direct-AST rejection and fallback JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies the direct Oxc
///   AST emitter declines the runtime-boundary quote body, then checks the
///   command-facing facade preserves the JS stub fallback.
#[test]
fn emit_core_module_with_oxc_codegen_falls_back_for_quote_expr() {
    let source = "\
module js_core_quote_fallback.

pub quoted(): Ast[Int] ->
    quote 1.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_quote_fallback.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    assert!(oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core).is_none());

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("fallback Oxc backend emits bootstrap JS");

    assert!(js.contains("export function quoted()"));
    assert!(
        js.contains("throw new Error(\"Terlan JS backend stub\")"),
        "{js}"
    );
}

/// Verifies that unquote expressions stay outside the direct JS backend
/// subset until macro-AST runtime semantics are selected.
///
/// Inputs:
/// - A checked Terlan module with a public function returning
///   `unquote(value)`.
///
/// Output:
/// - Assertions over direct-AST rejection and fallback JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies the direct Oxc
///   AST emitter declines the runtime-boundary unquote body, then checks
///   the command-facing facade preserves the JS stub fallback.
#[test]
fn emit_core_module_with_oxc_codegen_falls_back_for_unquote_expr() {
    let source = "\
module js_core_unquote_fallback.

pub unquoted(value: Int): Int ->
    unquote(value).
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_unquote_fallback.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    assert!(oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core).is_none());

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("fallback Oxc backend emits bootstrap JS");

    assert!(js.contains("export function unquoted(value)"));
    assert!(
        js.contains("throw new Error(\"Terlan JS backend stub\")"),
        "{js}"
    );
}

/// Verifies that inline HTML blocks stay outside the direct JS backend
/// subset until HTML rendering semantics are selected for `emit-js`.
///
/// Inputs:
/// - A checked Terlan module with a public function returning an
///   `html { ... }` block.
///
/// Output:
/// - Assertions over direct-AST rejection and fallback JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies the direct Oxc
///   AST emitter declines the runtime-boundary HTML block body, then
///   checks the command-facing facade preserves the JS stub fallback.
#[test]
fn emit_core_module_with_oxc_codegen_falls_back_for_html_block_expr() {
    let source = "\
module js_core_html_block_fallback.

pub view(): Html ->
    html { <main>Hello</main> }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_html_block_fallback.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    assert!(oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core).is_none());

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("fallback Oxc backend emits bootstrap JS");

    assert!(js.contains("export function view()"));
    assert!(
        js.contains("throw new Error(\"Terlan JS backend stub\")"),
        "{js}"
    );
}

/// Verifies that direct Oxc AST lowering handles tuple and list literals.
///
/// Inputs:
/// - A checked Terlan module with public tuple and list literal returns.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that CoreIR tuple/list
///   values use the JavaScript array representation.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_array_like_literals() {
    let source = "\
module js_core_direct_arrays.

pub pair(): {Int, Int} ->
    {1, 2}.

pub values(): List[Int] ->
    [3, 4].

pub fixed(): FixedArray[2, Int] ->
    #[5, 6].
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_arrays.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits tuple/list literal CoreIR");

    assert!(js.contains("export function pair()"));
    assert!(js.contains("return [1, 2];"), "{js}");
    assert!(js.contains("export function values()"));
    assert!(js.contains("return [3, 4];"), "{js}");
    assert!(js.contains("export function fixed()"));
    assert!(js.contains("return [5, 6];"), "{js}");
}

/// Verifies that direct Oxc AST lowering handles unary negation.
///
/// Inputs:
/// - A checked Terlan module with one public unary-minus function.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that CoreIR unary minus
///   becomes a JavaScript unary negation return.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_unary_negation() {
    let source = "\
module js_core_direct_unary.

pub negate(value: Int): Int ->
    -value.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_unary.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits unary negation CoreIR");

    assert!(js.contains("export function negate(value)"));
    assert!(
        js.contains("return -value;") || js.contains("return (-value);"),
        "{js}"
    );
}

/// Verifies that direct Oxc AST lowering handles expression-side list cons.
///
/// Inputs:
/// - A checked Terlan module with one public list-cons function.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that CoreIR list cons
///   becomes a JavaScript array literal with a spread tail.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_list_cons() {
    let source = "\
module js_core_direct_list_cons.

pub prepend(head: Int, tail: List[Int]): List[Int] ->
    [head | tail].
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_list_cons.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits list-cons CoreIR");

    assert!(js.contains("export function prepend(head, tail)"));
    assert!(js.contains("return [head, ...tail];"), "{js}");
}

/// Verifies that source index expressions currently fall back in Oxc JS.
///
/// Inputs:
/// - A checked Terlan module with one public fixed-array indexing
///   function.
///
/// Output:
/// - Assertions over direct-AST rejection and fallback JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, which lowers bracket
///   source syntax through `IndexGet.get_at`, verifies direct Oxc emission
///   declines that trait-backed call, then checks the public Oxc facade
///   returns the JS stub fallback.
#[test]
fn emit_core_module_with_oxc_codegen_falls_back_for_index_trait_call() {
    let source = "\
module js_core_direct_index.

pub first(items: FixedArray[2, Int]): Int ->
    items[0].
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_index.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    assert!(oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core).is_none());

    let js = oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("fallback Oxc backend emits bootstrap JS");

    assert!(js.contains("export function first(items)"));
    assert!(
        js.contains("throw new Error(\"Terlan JS backend stub\")"),
        "{js}"
    );
}

/// Verifies that direct Oxc AST lowering handles identifier-key map
/// literals.
///
/// Inputs:
/// - A checked Terlan module with one public map literal function.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that a CoreIR map literal
///   becomes a JavaScript object literal for the current identifier-key
///   subset.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_map_literal() {
    let source = "\
module js_core_direct_map.

pub point(): Term ->
    {x: 1, y: 2}.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_map.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits map literal CoreIR");

    assert!(js.contains("export function point()"));
    assert!(js.contains("return {"), "{js}");
    assert!(js.contains("x: 1"), "{js}");
    assert!(js.contains("y: 2"), "{js}");
}

/// Verifies that direct Oxc AST lowering handles struct field-access
/// expressions.
///
/// Inputs:
/// - A checked Terlan module with a public struct and field reader.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that CoreIR field access
///   becomes JavaScript static member access.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_field_access() {
    let source = "\
module js_core_direct_field.

pub struct Point {
    x: Int
}.

pub read(point: Point): Int ->
    point.x.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_field.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits field-access CoreIR");

    assert!(js.contains("export function read(point)"));
    assert!(js.contains("return point.x;"), "{js}");
}

/// Verifies that direct Oxc AST lowering handles struct construction.
///
/// Inputs:
/// - A checked Terlan module with a public struct constructor function.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that CoreIR record
///   construction becomes a JavaScript object literal for the current
///   struct-value subset.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_record_construct() {
    let source = "\
module js_core_direct_record_construct.

pub struct Point {
    x: Int
}.

pub make(): Point ->
    Point { x: 1 }.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_record_construct.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits record construction CoreIR");

    assert!(js.contains("export function make()"));
    assert!(js.contains("return {"), "{js}");
    assert!(js.contains("x: 1"), "{js}");
}

/// Verifies that direct Oxc AST lowering handles explicit record-access
/// expressions.
///
/// Inputs:
/// - A checked Terlan module with a public struct and explicit record
///   field reader.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that CoreIR record access
///   becomes JavaScript static member access.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_record_access() {
    let source = "\
module js_core_direct_record_access.

pub struct Point {
    x: Int
}.

pub read(point: Point): Int ->
    point#Point.x.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_record_access.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits record-access CoreIR");

    assert!(js.contains("export function read(point)"));
    assert!(js.contains("return point.x;"), "{js}");
}

/// Verifies that direct Oxc AST lowering covers selected binary operators.
///
/// Inputs:
/// - A checked Terlan module with public arithmetic and comparison
///   operator functions.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that selected CoreIR
///   binary operators map to their JavaScript operator forms.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_binary_operator_set() {
    let source = "\
module js_core_direct_binary_ops.

pub subtract(x: Int, y: Int): Int ->
    x - y.

pub multiply(x: Int, y: Int): Int ->
    x * y.

pub divide(x: Float, y: Float): Float ->
    x / y.

pub integer_divide(x: Int, y: Int): Int ->
    x div y.

pub remainder(x: Int, y: Int): Int ->
    x rem y.

pub same(x: Int, y: Int): Bool ->
    x == y.

pub exact_same(x: Int, y: Int): Bool ->
    x == y.

pub not_same(x: Int, y: Int): Bool ->
    x != y.

pub not_exact_same(x: Int, y: Int): Bool ->
    x != y.

pub less_than(x: Int, y: Int): Bool ->
    x < y.

pub less_than_or_equal(x: Int, y: Int): Bool ->
    x <= y.

pub greater_than(x: Int, y: Int): Bool ->
    x > y.

pub greater_than_or_equal(x: Int, y: Int): Bool ->
    x >= y.
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_binary_ops.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits selected binary operator CoreIR");

    assert!(js.contains("return x - y;"), "{js}");
    assert!(js.contains("return x * y;"), "{js}");
    assert!(js.contains("return x / y;"), "{js}");
    assert!(js.contains("return Math.trunc(x / y);"), "{js}");
    assert!(js.contains("return x % y;"), "{js}");
    assert!(js.contains("return x === y;"), "{js}");
    assert_eq!(js.matches("return x === y;").count(), 2, "{js}");
    assert_eq!(js.matches("return x !== y;").count(), 2, "{js}");
    assert!(js.contains("return x < y;"), "{js}");
    assert!(js.contains("return x <= y;"), "{js}");
    assert!(js.contains("return x > y;"), "{js}");
    assert!(js.contains("return x >= y;"), "{js}");
}

/// Verifies that direct Oxc AST lowering handles local named calls.
///
/// Inputs:
/// - A checked Terlan module with a private local function and a public
///   caller.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that a CoreIR local call
///   becomes a JavaScript call expression.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_named_call() {
    let source = "\
module js_core_direct_named_call.

identity(x: Int): Int ->
    x.

pub call_it(): Int ->
    identity(1).
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_named_call.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits named-call CoreIR");

    assert!(js.contains("function identity(x)"));
    assert!(!js.contains("export function identity"));
    assert!(js.contains("export function call_it()"));
    assert!(js.contains("return identity(1);"), "{js}");
}

/// Verifies that direct Oxc AST lowering handles function-value invocation.
///
/// Inputs:
/// - A checked Terlan module whose public function invokes a function-typed
///   parameter with `f(value)`.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, passes its `CoreModule`
///   into the direct Oxc AST emitter, and checks that
///   `CoreExpr::FunctionCall` becomes a JavaScript callable-value
///   application.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_function_value_call() {
    let source = "\
module js_core_direct_function_value_call.

pub apply(value: Int, f: (Int) -> Int): Int ->
    f(value).
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_function_value_call.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits function-value call CoreIR");

    assert!(js.contains("export function apply(value, f)"));
    assert!(js.contains("return f(value);"), "{js}");
}

/// Verifies that direct Oxc AST lowering handles selected CoreIR intrinsics.
///
/// Inputs:
/// - A checked Terlan module whose public function calls
///   `"hello".contains("ell")`.
///
/// Output:
/// - Assertions over Oxc-printed JavaScript source.
///
/// Transformation:
/// - Compiles source through the formal pipeline, verifies receiver-method
///   syntax lowers to the backend-neutral `core.string.contains` intrinsic,
///   and checks direct Oxc lowering emits JavaScript `.includes(...)`.
#[test]
fn emit_core_module_with_direct_oxc_ast_handles_string_contains_intrinsic() {
    let source = "\
module js_core_direct_string_contains_intrinsic.

pub has_needle(): Bool ->
    \"hello\".contains(\"ell\").
";
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_core_direct_string_contains_intrinsic.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile source to CoreIR");

    let js = oxc_backend::emit_core_module_with_direct_oxc_ast(&artifacts.core)
        .expect("direct Oxc AST emits string contains intrinsic CoreIR");

    assert!(js.contains("export function has_needle()"));
    assert!(js.contains(r#"return "hello".includes("ell");"#), "{js}");
}
