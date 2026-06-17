use std::collections::BTreeMap;

use terlan_hir::syntax_module_output_to_interface;
use terlan_syntax::{
    parse_interface_module_as_syntax_output, parse_module_as_syntax_output, SyntaxSourceKind,
};

#[test]
fn formal_syntax_output_direct_emit_preserves_type_and_function_docs() {
    let module = parse_module_as_syntax_output(
        r#"
//! Module docs.
//! Second module line.

module syntax_output_docs_emit.

/// Status value.
pub type Status = :ok.

/// Adds one.
pub add(x: Int): Int ->
x + 1.
"#,
    )
    .expect("parse syntax output docs fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("docs should lower directly from syntax output")
    .render();

    assert!(
        output.contains("-moduledoc \"Module docs.\\nSecond module line.\"."),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("-doc \"Status value.\".\n\n-type status() :: ok."),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("-doc \"Adds one.\".\n\n-spec add(integer()) -> integer()."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_preserves_struct_and_field_docs() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_struct_docs_emit.

/// A user account.
pub struct User {
/// Stable internal ID.
id: Int,

/// Display name.
name: Text
}.
"#,
    )
    .expect("parse syntax output struct docs fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("struct docs should lower directly from syntax output")
    .render();

    assert!(output.contains("-export_type([user/0])."));
    assert!(output.contains("-type user() :: #user{}."));
    assert!(output.contains("id % Stable internal ID."));
    assert!(output.contains("name % Display name."));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_alias_constructor_subset() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_emit.

pub type Ok[T] =
{:ok, value: T}.

pub make(value: Int): Dynamic ->
Ok(value).

pub unwrap(input: Dynamic): Dynamic ->
case input {
    Ok(value) -> value
}.
"#,
    )
    .expect("parse syntax output emit fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("formal subset should lower directly from syntax output")
    .render();

    assert!(output.contains("-type ok(T) :: {ok, T}."));
    assert!(output.contains("make(Value) ->\n    {ok, Value}."));
    assert!(output.contains("{ok, Value} -> Value"));
}

#[test]
fn formal_syntax_output_direct_emit_try_api_uses_direct_lowering() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_try_emit.

pub id(value: Int): Int ->
value.
"#,
    )
    .expect("parse syntax output try emit fixture");

    let output = super::try_emit_syntax_module_output_to_erlang(&module)
        .expect("formal try emit should lower directly from syntax output");

    assert!(output.contains("-module(syntax_output_try_emit)."));
    assert!(output.contains("-export([id/1])."));
    assert!(output.contains("id(Value) ->\n    Value."));
}

#[test]
fn formal_syntax_output_direct_emit_ignores_source_export_payloads() {
    let mut module = parse_interface_module_as_syntax_output(
        r#"
module syntax_output_source_export_payload.

export ghost/1.
"#,
    )
    .expect("parse interface export payload fixture");
    module.source_kind = SyntaxSourceKind::Module;

    let output = super::try_emit_syntax_module_output_to_erlang(&module)
        .expect("module-mode export payload should still lower as an empty module");

    assert!(output.contains("-module(syntax_output_source_export_payload)."));
    assert!(!output.contains("-export([ghost/1])."));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_if_expressions() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_if_emit.

pub choose(flag: Bool): Int ->
if {
    flag -> 1;
    true -> 0
}.
"#,
    )
    .expect("parse syntax output if fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("if expressions should lower directly from syntax output")
    .render();

    assert!(
        output.contains("choose(Flag) ->\n    if\n    Flag -> 1;\n    true -> 0\nend."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_try_expressions() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_try_expr_emit.

pub wait(): Int ->
try risky() {
    {:ok, value} -> value
catch
    :error -> 0
}.

risky(): {:ok, Int} ->
{:ok, 1}.
"#,
    )
    .expect("parse syntax output try expression fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("try expressions should lower directly from syntax output")
    .render();

    assert!(
        output.contains(
            "wait() ->\n    try risky()\nof\n    {ok, Value} -> Value\n\ncatch\n    error -> 0\nend."
        ),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_try_after_cleanup() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_try_after_emit.

pub wait(): Int ->
try risky() {
after
    0 -> 1
}.

risky(): Int ->
1.
"#,
    )
    .expect("parse syntax output try after expression fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    let module = output.expect("try-after should lower directly from syntax output");

    let source = module.render();
    assert!(source.contains("after\n    0 -> 1"), "output:\n{}", source);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_unary_expressions() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_unary_emit.

pub flip(flag: Bool): Bool ->
not flag.

pub negate(value: Int): Int ->
-value.
"#,
    )
    .expect("parse syntax output unary expression fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("unary expressions should lower directly from syntax output")
    .render();

    assert!(
        output.contains("flip(Flag) ->\n    not Flag."),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("negate(Value) ->\n    -Value."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_remote_fun_ref_source_syntax() {
    let parsed = parse_module_as_syntax_output(
        r#"
module syntax_output_remote_fun_ref_emit.

pub ref(): Dynamic ->
fun math:double/1.
"#,
    );

    assert!(
        parsed.is_err(),
        "remote fun references are backend output syntax, not canonical Terlan source"
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_macro_exprs() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_macro_emit.

pub module_name(): Dynamic ->
?MODULE.

pub compare(a: Int, b: Int): Dynamic ->
?assert_equal(a, b).
"#,
    )
    .expect("parse syntax output macro expr fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("macro exprs should lower directly from syntax output")
    .render();

    assert!(
        output.contains("module_name() ->\n    ?MODULE."),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("compare(A, B) ->\n    ?assert_equal(A, B)."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_raw_macro_exprs_without_resolution() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_raw_macro_emit.

pub query(): Dynamic ->
sql{select * from users}.
"#,
    )
    .expect("parse syntax output raw macro expr fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "raw macro expr should require macro resolution before direct emit"
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_constructor_chain() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_constructor_chain_emit.

pub type User = Dynamic.
pub constructor User {
(id: Int, name: Binary): Dynamic ->
    id
}.

pub demo(id: Int, name: Binary): Dynamic ->
User(id, name) with Admin { id = id, name = name }.
"#,
    )
    .expect("parse syntax output constructor chain fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_some(),
        "constructor chain should lower during direct syntax emit"
    );

    let source = output
        .as_ref()
        .expect("output exists because test expects lowering")
        .render();
    assert!(
        source.contains("begin\n"),
        "expected constructor chain to lower to sequenced derived shape: {}",
        source
    );
    assert!(
        source.contains("{'Admin', Id, Name}"),
        "expected constructed derived tuple to be emitted: {}",
        source
    );
    assert!(
        !source.contains("#admin"),
        "constructor extension must not emit undeclared Erlang records: {}",
        source
    );
}

#[test]
fn formal_syntax_output_direct_emit_maps_binary_ops_without_ast_enum() {
    assert_eq!(super::lower_syntax_binary_op_render("+"), "+");
    assert_eq!(super::lower_syntax_binary_op_render("=="), "=:=");
    assert_eq!(super::lower_syntax_binary_op_render("=:="), "=:=");
    assert_eq!(super::lower_syntax_binary_op_render("<="), "=<");
    assert_eq!(super::lower_syntax_binary_op_render("div"), "div");
    assert_eq!(super::lower_syntax_binary_op_render("!"), "!");
}

#[test]
fn formal_syntax_output_direct_emit_lowers_pipe_forward() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_pipe_emit.

pub add(value: Int, amount: Int): Int ->
value + amount.

pub demo(value: Int): Int ->
value |> add(1).
"#,
    )
    .expect("parse syntax output pipe fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("pipe subset should lower directly from syntax output")
    .render();

    assert!(output.contains("demo(Value) ->\n    add(Value, 1)."));
    assert!(!output.contains("|>"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_keyword_expr_pipe_forward() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_keyword_pipe_emit.

pub inspect(value: Int): Int ->
value.

pub demo(option: Dynamic): Int ->
case option {
    :none -> 0;
    value -> value
} |> inspect().
"#,
    )
    .expect("parse syntax output keyword pipe fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("keyword pipe should lower directly from syntax output")
    .render();

    assert!(
        output.contains("demo(Option) ->\n    inspect(case Option of"),
        "output:\n{}",
        output
    );
    assert!(!output.contains("|>"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_function_clause_guards() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_function_guard_emit.

pub abs(value) when value < 0 ->
0 - value;
abs(value) ->
value.
"#,
    )
    .expect("parse syntax output function guard fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("function guards should lower directly from syntax output")
    .render();

    assert!(
        output.contains("abs(Value) when Value < 0 ->\n    0 - Value;"),
        "output:\n{}",
        output
    );
    assert!(output.contains("abs(Value) ->\n    Value."));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_case_guards() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_case_guard_emit.

pub classify(value: Int): Int ->
case value {
    x when x > 0 -> x;
    _ -> 0
}.
"#,
    )
    .expect("parse syntax output case guard fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("case guard should lower directly from syntax output")
    .render();

    assert!(output.contains("X when X > 0 -> X"), "output:\n{}", output);
    assert!(output.contains("_ -> 0"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_raw_atom_patterns() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_raw_atom_pattern_emit.

pub classify(value: Dynamic): Dynamic ->
case value {
    :none -> :ok;
    :empty -> :ok;
    other -> other
}.
"#,
    )
    .expect("parse syntax output raw atom pattern fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("raw atom patterns should lower directly from syntax output")
    .render();

    assert!(output.contains("'none' -> ok"), "output:\n{}", output);
    assert!(output.contains("'empty' -> ok"), "output:\n{}", output);
    assert!(output.contains("Other -> Other"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_quoted_atom_exprs_and_patterns() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_quoted_atom_emit.

pub module_atom(): Dynamic ->
:'Elixir.Module'.

pub classify(value: Dynamic): Dynamic ->
case value {
    :'some atom' -> :ok;
    :none -> :ok
}.
"#,
    )
    .expect("parse syntax output quoted atom fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("quoted atoms should lower directly from syntax output")
    .render();

    assert!(
        output.contains("module_atom() ->\n    'Elixir.Module'."),
        "output:\n{}",
        output
    );
    assert!(output.contains("'some atom' -> ok"), "output:\n{}", output);
    assert!(output.contains("'none' -> ok"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_bool_literals_and_patterns() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_bool_literal_emit.

pub negate(value: Bool): Bool ->
case value {
    true -> false;
    false -> true
}.
"#,
    )
    .expect("parse syntax output bool literal fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("bool literals should lower directly from syntax output")
    .render();

    assert!(output.contains("true -> false"), "output:\n{}", output);
    assert!(output.contains("false -> true"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_module_alias_remote_calls() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_module_alias.

	import std.collections.queue as queue.

pub len_is_zero(): Bool ->
queue.len(queue.empty()) == 0.
"#,
    )
    .expect("parse syntax output module alias fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("module alias remote call should lower directly from syntax output")
    .render();

    assert!(
        output.contains("std_collections_queue:len(std_collections_queue:empty())"),
        "output:\n{}",
        output
    );
}

/// Verifies local receiver-method calls do not lower through the remote-call
/// syntax bridge path.
///
/// Inputs:
/// - A formal syntax-output module containing `user.display_name()`, where
///   `user` is a function parameter.
///
/// Output:
/// - Test passes when direct Erlang lowering rejects the module because
///   semantic receiver-method resolution has not run.
///
/// Transformation:
/// - Parses the canonical method-call suffix and checks that the Erlang
///   syntax bridge treats local receivers differently from module
///   roots.
#[test]
fn formal_syntax_output_direct_emit_does_not_lower_local_receiver_method_as_remote_call() {
    let module = parse_module_as_syntax_output(
        r#"
module local_receiver_method_shape.

pub render(user: User): String ->
user.display_name().
"#,
    )
    .expect("parse local receiver method-shaped call fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(output.is_none());
}

/// Verifies Erlang syntax lowering preserves receiver mutability metadata.
///
/// Inputs:
/// - A syntax-output module containing one mutable receiver method and one
///   immutable receiver method for the same receiver type.
///
/// Output:
/// - Test passes when the lowering context marks only the mutable method as
///   mutable while preserving ordinary receiver-method lookup behavior.
///
/// Transformation:
/// - Builds the direct syntax-output lowering context and inspects the internal
///   receiver-method inventory that later mutable rebinding lowering will use.
#[test]
fn formal_syntax_output_lowering_context_preserves_receiver_mutability() {
    let module = parse_module_as_syntax_output(
        r#"
module receiver_mutability_context.

pub struct Map {
    size: Int
}.

pub (mut map: Map) put(): Unit ->
    Unit.

pub (map: Map) size(): Int ->
    map.size.
"#,
    )
    .expect("parse receiver mutability context fixture");

    let ctx = super::syntax::SyntaxLowerCtx::new(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        ctx.receiver_method_target("Map", "put", 0)
            .expect("put receiver target")
            .mutable
    );
    assert!(
        !ctx.receiver_method_target("Map", "size", 0)
            .expect("size receiver target")
            .mutable
    );
}

/// Verifies mutable receiver pipes bind the backend-updated receiver.
///
/// Inputs:
/// - A syntax-output module with one mutable receiver method and one function
///   using `map |> put()`.
///
/// Output:
/// - Test passes when direct Erlang lowering emits a backend-local binding from
///   `put(Map)` and returns that binding as the pipe expression value.
///
/// Transformation:
/// - Runs the syntax-output Erlang bridge directly, bypassing formal pipeline
///   typecheck gates, and inspects the rendered Erlang source for the hidden
///   mutable receiver threading convention.
#[test]
fn formal_syntax_output_direct_emit_lowers_mutable_receiver_pipe_to_updated_binding() {
    let module = parse_module_as_syntax_output(
        r#"
module mutable_receiver_pipe_emit.

pub struct Map {
    size: Int
}.

pub (mut map: Map) put(): Unit ->
    map.

pub run(map: Map): Map ->
    map |> put().
"#,
    )
    .expect("parse mutable receiver pipe fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("mutable receiver pipe should lower directly from syntax output")
    .render();

    assert!(
        output.contains("_TerlanMutReceiver = put(Map)"),
        "output:\n{}",
        output
    );
    assert!(
        output.contains(
            "run(Map) ->\n    begin\n    _TerlanMutReceiver = put(Map),\n    _TerlanMutReceiver\nend."
        ),
        "output:\n{}",
        output
    );
    assert!(!output.contains("|>"), "output:\n{}", output);
}

/// Verifies sequences thread mutable receiver updates into later expressions.
///
/// Inputs:
/// - A syntax-output module with a mutable receiver method, an immutable
///   receiver method, and a sequence `map.put(); map.size()`.
///
/// Output:
/// - Test passes when the mutable call result is bound once and the following
///   receiver method call uses the updated binding instead of the original
///   source parameter.
///
/// Transformation:
/// - Runs direct syntax-output Erlang lowering and inspects the rendered
///   sequence binding convention used before formal build gates enable mutable
///   receiver execution.
#[test]
fn formal_syntax_output_direct_emit_lowers_mutable_receiver_sequence_to_rebinding() {
    let module = parse_module_as_syntax_output(
        r#"
module mutable_receiver_sequence_emit.

pub struct Map {
    size: Int
}.

pub (mut map: Map) put(): Unit ->
    map.

pub (map: Map) size(): Int ->
    map.size.

pub run(map: Map): Int ->
    map.put(); map.size().
"#,
    )
    .expect("parse mutable receiver sequence fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("mutable receiver sequence should lower directly from syntax output")
    .render();

    assert!(
        output.contains("_TerlanMutReceiver0 = put(Map)"),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("size(_TerlanMutReceiver0)"),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_qualified_type_specs() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_qualified_specs.

pub id(value: users.UserId): users.UserId ->
value.

pub boxed(value: users.Box[Int]): users.Box[Int] ->
value.

pub comparison(value: std.core.Ordering.Comparison): std.core.Ordering.Comparison ->
value.
"#,
    )
    .expect("parse syntax output qualified type fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("qualified type specs should lower directly from syntax output")
    .render();

    assert!(output.contains("-spec id(users:user_id()) -> users:user_id()."));
    assert!(output.contains("-spec boxed(users:box(integer())) -> users:box(integer())."));
    assert!(output.contains(
        "-spec comparison(std_core_ordering:comparison()) -> std_core_ordering:comparison()."
    ));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_dotted_modules_and_remote_calls() {
    let module = parse_module_as_syntax_output(
        r#"
module std.collections.queue.tests.

pub len_is_zero(): Bool ->
std.collections.queue.len(std.collections.queue.empty()) == 0.
"#,
    )
    .expect("parse syntax output dotted module fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("dotted modules should lower directly from syntax output")
    .render();

    assert!(output.contains("-module(std_collections_queue_tests)."));
    assert!(output.contains("std_collections_queue:len(std_collections_queue:empty())"));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_colon_remote_calls() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_colon_remote_emit.

pub show(): Dynamic ->
io_lib:format("~p", []).
"#,
    )
    .expect("parse syntax output colon remote call fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("colon remote calls should lower directly from syntax output")
    .render();

    assert!(
        output.contains("show() ->\n    io_lib:format(\"~p\", [])."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_opaque_constructors_and_phantom_specs() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_queue_specs.

pub opaque type Queue[T] =
Term.

pub empty(): Queue[T] ->
Queue(queue.new()).

pub from_term(value: Term): Queue[T] ->
Queue(value).

pub len(queue_value: Queue[T]): Int ->
queue.len(queue_value).
"#,
    )
    .expect("parse syntax output opaque queue fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("opaque constructors should lower directly from syntax output")
    .render();

    assert!(output.contains("-opaque queue(_T) :: term()."));
    assert!(output.contains("-spec from_term(term()) -> queue(_T)."));
    assert!(output.contains("-spec len(queue(_T)) -> integer()."));
    assert!(output.contains("empty() ->\n    queue:new()."));
    assert!(output.contains("from_term(Value) ->\n    Value."));
    assert!(!output.contains("Queue("), "output:\n{}", output);
}

/// Verifies empty opaque declarations emit valid Erlang placeholder specs.
///
/// Inputs:
/// - A syntax-output module containing `pub opaque type Iterator[T].`.
///
/// Output:
/// - Test passes when the rendered Erlang module uses `term()` as the hidden
///   opaque representation and keeps the phantom type parameter valid.
///
/// Transformation:
/// - Exercises the backend rule that representation-hidden Terlan opaque
///   types are hidden from source users but still require a concrete Erlang
///   right-hand side for `erlc`.
#[test]
fn formal_syntax_output_direct_emit_lowers_empty_opaque_type_specs() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_empty_opaque_specs.

pub opaque type Iterator[T].
"#,
    )
    .expect("parse empty opaque type fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("empty opaque type should lower directly from syntax output")
    .render();

    assert!(output.contains("-opaque iterator(_T) :: term()."));
}

#[test]
fn formal_syntax_output_direct_emit_rejects_local_opaque_constructor_patterns() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_local_opaque_pattern_emit.

pub opaque type UserId =
Int.

pub unwrap(input: UserId): Int ->
case input {
    UserId(value) -> value
}.
"#,
    )
    .expect("parse local opaque pattern fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "local opaque constructor patterns should not lower directly"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_imported_opaque_constructor_calls() {
    let provider = parse_module_as_syntax_output(
        r#"
module users.

pub opaque type UserId =
Int.
"#,
    )
    .expect("parse opaque provider");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module syntax_output_imported_opaque_emit.

import users.{UserId}.

pub make(value: Int): UserId ->
UserId(value).
"#,
    )
    .expect("parse imported opaque consumer");

    let output = super::lower_syntax_module_output(
        &consumer,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "imported opaque constructor calls should not lower directly"
    );
}

#[test]
fn formal_syntax_output_direct_emit_rejects_imported_opaque_constructor_patterns() {
    let provider = parse_module_as_syntax_output(
        r#"
module users.

pub opaque type UserId =
Int.
"#,
    )
    .expect("parse opaque provider");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module syntax_output_imported_opaque_pattern_emit.

import users.{UserId}.

pub unwrap(input: UserId): Int ->
case input {
    UserId(value) -> value
}.
"#,
    )
    .expect("parse imported opaque pattern consumer");

    let output = super::lower_syntax_module_output(
        &consumer,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    );

    assert!(
        output.is_none(),
        "imported opaque constructor patterns should not lower directly"
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_maps_and_funs() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_container_emit.

pub make(value: Int): Dynamic ->
#{count => value, ok = :ok}.

pub pick(input: Dynamic): Dynamic ->
case input {
    #{count = value} -> value
}.

pub mapper(): Dynamic ->
(value) -> value + 1.
"#,
    )
    .expect("parse syntax output container fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("container subset should lower directly from syntax output")
    .render();

    assert!(
        output.contains("make(Value) ->\n    #{count=>Value, ok=>ok}."),
        "output:\n{}",
        output
    );
    assert!(output.contains("#{count:=Value} -> Value"));
    assert!(output.contains("mapper() ->\n    fun\n    (Value) -> Value + 1\nend."));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_map_exprs_and_patterns() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_map_emit.

pub make(value: Int): Dynamic ->
#{count => value, ok = :ok}.

pub pick(input: Dynamic): Dynamic ->
case input {
    #{count = value} -> value
}.
"#,
    )
    .expect("parse syntax output map fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("map expressions and patterns should lower directly from syntax output")
    .render();

    assert!(
        output.contains("make(Value) ->\n    #{count=>Value, ok=>ok}."),
        "output:\n{}",
        output
    );
    assert!(output.contains("#{count:=Value} -> Value"));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_anonymous_fun_expressions() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_fun_emit.

pub mapper(): Dynamic ->
(value) -> value + 1.
"#,
    )
    .expect("parse syntax output anonymous fun fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("anonymous fun should lower directly from syntax output")
    .render();

    assert!(
        output.contains("mapper() ->\n    fun\n    (Value) -> Value + 1\nend."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_fixed_array_indexes() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_fixed_array_index_emit.

pub second(): Dynamic ->
#[1, 2, 3][1].
"#,
    )
    .expect("parse syntax output fixed array index fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("fixed array index should lower directly from syntax output")
    .render();

    assert!(
        output.contains("second() ->\n    element((1) + 1, {1, 2, 3})."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_list_comprehensions() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_list_comprehension_emit.

pub increment(values: List[Int]): List[Int] ->
[value + 1 | value <- values].
"#,
    )
    .expect("parse syntax output list comprehension fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("list comprehension should lower directly from syntax output")
    .render();

    assert!(
        output.contains("increment(Values) ->\n    [Value + 1 || Value <- Values]."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_stacked_list_comprehension_filters() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_list_comprehension_filter_emit.

pub selected(values: List[Int]): List[Int] ->
[value | value <- values, value > 0, value < 10].
"#,
    )
    .expect("parse stacked-filter list comprehension fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("stacked-filter list comprehension should lower directly from syntax output")
    .render();

    assert!(
        output.contains(
            "selected(Values) ->\n    [Value || Value <- Values, Value > 0 andalso Value < 10]."
        ),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_list_cons_exprs_and_patterns() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_list_cons_emit.

pub prepend(head: Int, tail: List[Int]): List[Int] ->
[head | tail].

pub first(list: List[Int]): Int ->
case list {
    [head | _tail] -> head
}.
"#,
    )
    .expect("parse syntax output list cons fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("list cons subset should lower directly from syntax output")
    .render();

    assert!(
        output.contains("prepend(Head, Tail) ->\n    [Head|Tail]."),
        "output:\n{}",
        output
    );
    assert!(
        output.contains("[Head|_tail] -> Head"),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_record_constructs() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_record_construct_emit.

pub make(id: Int, name: Text): Dynamic ->
#User{id = id, name = name}.
"#,
    )
    .expect("parse syntax output record construct fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("record construct should lower directly from syntax output")
    .render();

    assert!(
        output.contains("make(Id, Name) ->\n    #user{id = Id, name = Name}."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_quote_and_unquote() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_quote_emit.

pub quoted(value: Int): Dynamic ->
quote unquote(value).
"#,
    )
    .expect("parse syntax output quote fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("quote subset should lower directly from syntax output")
    .render();

    assert!(
        output.contains("quoted(Value) ->\n    quote unquote(Value)."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_structs_with_defaults() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_struct_emit.

pub struct User {
id: Int,
name: Text,
status: Dynamic = :active
}.

pub make(id: Int, name: Text): User ->
#User{id = id, name = name}.
"#,
    )
    .expect("parse syntax output struct fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("struct subset should lower directly from syntax output")
    .render();

    assert!(output.contains("-export_type([user/0])."));
    assert!(output.contains("-type user() :: #user{}."));
    assert!(
        output.contains("-record(user, {id, name, status = 'active'})."),
        "output:\n{}",
        output
    );
    assert!(output.contains("make(Id, Name) ->\n    #user{id = Id, name = Name}."));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_struct_field_access_from_param_type() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_struct_field_emit.

pub struct User {
id: Int,
name: Text
}.

pub username(user: User): Text ->
user.name.
"#,
    )
    .expect("parse syntax output struct field fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("struct field access should lower directly from syntax output")
    .render();

    assert!(output.contains("username(User) ->\n    User#user.name."));
    assert!(
        output.find("-record(user").unwrap_or(usize::MAX)
            < output.find("-type user").unwrap_or(usize::MAX),
        "record declarations must appear before types that reference them:\n{}",
        output
    );
    assert!(!output.contains("User#name.name"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_record_updates() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_record_update_emit.

pub struct User {
id: Int,
name: Text
}.

pub rename(user: User, name: Text): User ->
user#User{name = name}.
"#,
    )
    .expect("parse syntax output record update fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("record update should lower directly from syntax output")
    .render();

    assert!(
        output.contains("rename(User, Name) ->\n    User#user{name = Name}."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_record_patterns() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_record_pattern_emit.

pub struct User {
id: Int,
name: Text
}.

pub username(user: User): Text ->
case user {
    #User{name = name} -> name
}.
"#,
    )
    .expect("parse syntax output record pattern fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("record pattern should lower directly from syntax output")
    .render();

    assert!(
        output.contains("#user{name = Name} -> Name"),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_struct_headers() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_struct_header_emit.

/// A user account.
pub struct User {
id: Int,
name: Text = <<"guest">>
}.
"#,
    )
    .expect("parse syntax output struct header fixture");

    let output = super::lower_syntax_struct_headers_to_hrl(&module)
        .expect("struct headers should lower directly from syntax output");

    assert!(output.contains("-record(user, {id, name = <<\"guest\">>})."));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_explicit_constructors() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_constructor_emit.

pub type Range =
Dynamic.

pub constructor Range {
(start: Int, stop: Int, step: Int = 1): Range ->
    {:range, start, stop, step}
}.

pub make(start: Int, stop: Int): Range ->
Range(start, stop).

pub first(value: Range): Int ->
case value {
    Range(start, stop, step) -> start
}.
"#,
    )
    .expect("parse syntax output constructor fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("explicit constructor subset should lower directly from syntax output")
    .render();

    assert!(output.contains("-export([first/1, make/2, typer_ctor_range_3/3])."));
    assert!(
        output.contains(
            "typer_ctor_range_3(Start, Stop, Step) ->\n    {'range', Start, Stop, Step}."
        ),
        "output:\n{}",
        output
    );
    assert!(output.contains("make(Start, Stop) ->\n    typer_ctor_range_3(Start, Stop, 1)."));
    assert!(output.contains("{'range', Start, Stop, Step} -> Start"));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_varargs_constructors() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_constructor_varargs_emit.

pub type Items[T] =
List[T].

pub constructor Items[T] {
(...values: T): Items[T] ->
    values
}.

pub from_args(a: Int, b: Int): Items[Int] ->
Items(a, b).
"#,
    )
    .expect("parse syntax output varargs constructor fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("varargs constructor subset should lower directly from syntax output")
    .render();

    assert!(output.contains("-export([from_args/2, typer_ctor_items_varargs_0/1])."));
    assert!(output.contains("-spec typer_ctor_items_varargs_0([T]) -> items(T)."));
    assert!(output.contains("typer_ctor_items_varargs_0(Values) ->\n    Values."));
    assert!(
        output.contains("from_args(A, B) ->\n    typer_ctor_items_varargs_0([A, B])."),
        "output:\n{}",
        output
    );
}

#[test]
fn formal_syntax_output_direct_emit_lowers_constructor_field_access_from_param_type() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_constructor_field_emit.

pub struct User {
name: Text
}.

pub type Named =
Dynamic.

pub constructor Named {
(user: User): Named ->
    {:named, user.name}
}.
"#,
    )
    .expect("parse syntax output constructor field fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("constructor field access should lower directly from syntax output")
    .render();

    assert!(
        output.contains("typer_ctor_named_1(User) ->\n    {'named', User#user.name}."),
        "output:\n{}",
        output
    );
    assert!(!output.contains("User#name.name"), "output:\n{}", output);
}

#[test]
fn formal_syntax_output_direct_emit_lowers_native_raw_declarations() {
    let module = parse_module_as_syntax_output(
        r#"
module syntax_output_native_emit.

native core module ArrayNative {
#[native(normal)]
length[T](value: Array[T]): Int.
}
"#,
    )
    .expect("parse syntax output native raw fixture");

    let output = super::lower_syntax_module_output(
        &module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("native raw subset should lower directly from syntax output")
    .render();

    assert!(output.contains("-export([length/1])."));
    assert!(output.contains("-on_load(load/0)."));
    assert!(output.contains("\"ArrayNative.so\""));
    assert!(output.contains("length(A1) ->\n    erlang:nif_error(nif_not_loaded)."));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_selected_imported_functions_as_remote_calls() {
    let provider = parse_module_as_syntax_output(
        r#"
module z_dep.

pub add(x: Int): Int ->
x + 1.
"#,
    )
    .expect("parse imported function provider");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module a_user.

import z_dep.{add}.

pub value(): Int ->
add(1).
"#,
    )
    .expect("parse imported function consumer");

    let output = super::lower_syntax_module_output(
        &consumer,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("selected imported function should lower directly from syntax output")
    .render();

    assert!(output.contains("value() ->\n    z_dep:add(1)."));
    assert!(!output.contains("value() ->\n    add(1)."));
}

#[test]
fn formal_syntax_output_direct_emit_lowers_imported_alias_constructor_subset() {
    let provider = parse_module_as_syntax_output(
        r#"
module result.

pub type Ok[T] =
{:ok, value: T}.
"#,
    )
    .expect("parse result provider");
    let mut interfaces = BTreeMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module result_user.

import result.{Ok}.

pub make(value: Int): Dynamic ->
Ok(value).

pub unwrap(input: Dynamic): Dynamic ->
case input {
    Ok(value) -> value
}.
"#,
    )
    .expect("parse result consumer");

    let output = super::lower_syntax_module_output(
        &consumer,
        &interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("imported alias subset should lower directly from syntax output")
    .render();

    assert!(output.contains("make(Value) ->\n    {ok, Value}."));
    assert!(output.contains("{ok, Value} -> Value"));
    assert!(!output.contains("typer_ctor_ok"));
}
