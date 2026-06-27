use std::collections::BTreeMap;

use crate::terlan_hir::syntax_module_output_to_interface;
use crate::terlan_syntax::{
    parse_interface_module_as_syntax_output, parse_module_as_syntax_output, SyntaxSourceKind,
};

/// Renders a lowered binary operator for emit tests.
///
/// Inputs:
/// - `operator`: source operator text.
///
/// Output:
/// - Erlang render spelling.
///
/// Transformation:
/// - Lowers the token through production operator mapping and calls the Erlang
///   operator renderer.
fn lower_syntax_binary_op_render(operator: &str) -> &'static str {
    super::lower_syntax_binary_op(Some(operator)).render()
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
    assert_eq!(lower_syntax_binary_op_render("+"), "+");
    assert_eq!(lower_syntax_binary_op_render("=="), "=:=");
    assert_eq!(lower_syntax_binary_op_render("=:="), "=:=");
    assert_eq!(lower_syntax_binary_op_render("<="), "=<");
    assert_eq!(lower_syntax_binary_op_render("div"), "div");
    assert_eq!(lower_syntax_binary_op_render("!"), "!");
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
