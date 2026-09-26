//! Source constructor bodies must execute, not merely select a field layout.

use super::native_object_test_support::{
    assert_managed_native_object_invocations, NativeObjectInvocation,
};
use super::{emit_native_application_object, status, NativeModule};
use crate::terlan_hir::{
    checked_in_std_interfaces_for_module, resolve_syntax_module_output_with_interfaces,
    syntax_module_output_to_interface,
};
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output};

#[test]
fn completed_guard_values_do_not_admit_unreachable_effect_adapters() {
    check_sources(&[
        r#"
module completed_guard_values.
import std.core.GuardResult.{accept, reject, value}.
pub check(): Bool ->
    value(accept()) and not value(reject()).
"#,
        include_str!("../../../../../std/core/GuardResult.terl"),
        include_str!("../../../../../std/core/Effect.terl"),
    ]);
}

#[test]
fn empty_byte_operands_adapt_without_losing_suspending_producers() {
    check_sources(&[r#"
module empty_byte_operands.
import std.vm.{Bytes, Process}.
pub produce(): List[Never] -> let _parked = Process.yield_now(); [].
pub check(): Bool ->
    let values = [];
    Bytes.from_list([]).length() == 0
        and Bytes.from_list(values).length() == 0
        and Bytes.from_list(produce()).length() == 0
        and Bytes.from_list([42]).length() == 1.
"#]);
}

#[test]
fn unconstrained_empty_maps_retain_uninhabited_slots() {
    check_sources(&[
        r#"
module empty_map_bottom.
import std.collections.Map.
pub check(): Bool ->
    let values = Map.new();
    let alias = values;
    alias.is_empty() and values.size() == 0.
"#,
        include_str!("../../../../../std/collections/Map.terl"),
    ]);
}

#[test]
fn empty_map_observation_preserves_later_mutation() {
    check_sources(&[
        r#"
module empty_map_mutation.
import std.collections.Map.
pub check(): Bool ->
    let values = Map.new();
    let before = values.is_empty();
    values.put("first", 42);
    before and values.size() == 1 and values.contains_key("first").
"#,
        include_str!("../../../../../std/collections/Map.terl"),
    ]);
}

#[test]
fn qualified_option_payload_retains_managed_receiver_type() {
    let source = r#"
module qualified_option_payload.
import std.core.Option.{Some, None}.
import std.vm.Bytes.
make(): std.core.Option.Option[std.vm.Bytes.Bytes] -> Some(Bytes.from_list([1, 2])).
pub check(): Bool -> case make() { Some(value) -> value.length() == 2; None -> false }.
"#;
    check_sources(&[source]);
    check_sources(&[source, include_str!("../../../../../std/core/Option.terl")]);
}

#[test]
fn tuple_option_patterns_retain_scalar_payloads_in_boolean_bindings() {
    check_sources(&[
        r#"
module tuple_option_payload.
import std.collections.List.
import std.core.Option.{None, Some}.
pub check(): Bool ->
    let values = List(1, 2, 1, 3);
    let result = values.subtract(List(1, 3));
    case {result.first(), values.first()} {
        {None, _} -> false;
        {_, None} -> false;
        {Some(result_first), Some(source_first)} ->
            let result_is_expected = result.length() == 2 and result_first == 2;
            let source_is_unchanged = values.length() == 4 and source_first == 1;
            result_is_expected and source_is_unchanged
    }.
"#,
        include_str!("../../../../../std/core/Option.terl"),
        include_str!("../../../../../std/collections/List.terl"),
    ]);
}

#[test]
fn qualified_list_parameters_share_literal_and_rest_storage() {
    check_sources(&[
        r#"
module qualified_list_storage.
import std.collections.List.
import std.core.Option.{None, Some}.
tail_starts_with_two(values: std.collections.List.List[Int]): Bool ->
    case values.first() { Some(first) -> values.length() == 2 and first == 2; None -> false }.
nested(values: std.collections.List.List[std.collections.List.List[Int]]): Bool ->
    case values.first() { Some(first) -> tail_starts_with_two(first); None -> false }.
pub check(): Bool ->
    nested(List(List(2, 3))) and
    (case List(1, 2, 3).rest() { Some(tail) -> tail_starts_with_two(tail); None -> false }).
"#,
        include_str!("../../../../../std/core/Option.terl"),
        include_str!("../../../../../std/collections/List.terl"),
    ]);
}

#[test]
fn imported_record_alias_retains_declared_layout_identity() {
    check_sources(&[
        r#"
module record_alias.
import std.collections.List.
import app.Containers.{List as Renamed}.
import app.Other.{List as OtherList}.
pub check(): Bool ->
    let boxed = Renamed(value = 42);
    let other = OtherList(label = "safe");
    boxed.value == 42 and other.label == "safe" and List(1, 2).length() == 2.
"#,
        r#"
module app.Containers.
pub struct List { value: Int }.
"#,
        r#"
module app.Other.
pub struct List { label: String }.
"#,
        include_str!("../../../../../std/core/Option.terl"),
        include_str!("../../../../../std/collections/List.terl"),
    ]);
}

#[test]
fn unconstrained_empty_lists_retain_the_checked_bottom_type() {
    check_sources(&[r#"
module empty_list_bottom.
import std.collections.List.
import std.core.Option.{None, Some}.
observe[T](values: List[T], _predicate: (T) -> Bool): Bool -> values.is_empty().
choose[T](values: List[T], value: T): T ->
    case values.first() { Some(first) -> first; None -> value }.
pub check(): Bool ->
    let values = [];
    values.length() == 0
    and (case values.first() { None -> true; Some(_) -> false })
    and observe([], (value) -> value == value)
    and choose([], 42) == 42
    and choose([], "value") == "value".
"#]);
}

#[test]
fn empty_binding_adopts_its_generic_consumer_and_later_mutation() {
    check_sources(&[r#"
module empty_binding_context.
import std.collections.List.
import std.core.Option.{None, Some}.
choose[T](values: List[T], value: T): T ->
    case values.first() { Some(first) -> first; None -> value }.
pub check(): Bool ->
    let values = [];
    let mutating = [];
    let initial_size = mutating.length();
    mutating.push(42);
    choose(values, 42) == 42 and initial_size == 0 and mutating.length() == 1.
"#]);
}

#[test]
fn empty_list_values_retain_independent_consumer_layouts() {
    check_sources(&[r#"
module empty_bottom_reuse.
import std.collections.List.
import std.core.Option.{None, Some}.
choose[T](values: List[T], value: T): T ->
    case values.first() { Some(first) -> first; None -> value }.
pub returned(values: List[Never]): List[Int] -> values.
pub produced(): List[Never] -> [].
pub returned_expression(): List[String] -> produced().
pub check(): Bool ->
    let values = [];
    let created = List.new();
    let output = produced();
    choose(values, 42) == 42
    and choose(values, "value") == "value"
    and choose(created, 42) == 42
    and choose(output, 42) == 42
    and choose(output, "value") == "value"
    and choose(produced(), 42) == 42
    and returned(values).is_empty()
    and returned_expression().is_empty().
"#]);
}

#[test]
fn empty_list_read_before_mutation_keeps_its_original_type() {
    check_sources(&[r#"
module empty_reuse_flow.
import std.collections.List.
import std.core.Option.{None, Some}.
choose[T](values: List[T], value: T): T ->
    case values.first() { Some(first) -> first; None -> value }.
pub check(): Bool ->
    let values = [];
    let before = choose(values, "empty");
    values.push(42);
    before == "empty" and choose(values, 7) == 42.
"#]);
}

#[test]
fn nested_empty_literals_share_the_concrete_sibling_layout() {
    check_sources(&[r#"
module nested_empty_bottom.
import std.collections.List.
import std.core.Option.{None, Some}.
pub check(): Bool ->
    let values = [[], [42]];
    values[0].is_empty()
    and (case values[1].first() { Some(value) -> value == 42; None -> false }).
"#]);
}

#[test]
fn wrapped_named_callable_retains_signature_and_lexical_shadowing() {
    check_sources(&[
        r#"
module wrapped_named_callable.
import std.core.Option.{Option, Some, None}.
increment(value: Int): Int -> value + 1.
use[A, B](value: A, transform: (A) -> B): B -> transform(value).
wrapped[A, B](value: Option[(A) -> B], argument: A): Option[B] ->
    case value { Some(transform) -> Some(transform(argument)); None -> None }.
shadowed(increment: (String) -> Int): Int -> use("hello", increment).
pub check(): Bool -> (case wrapped(Some(increment), 41) { Some(result) -> result == 42; None -> false })
    and shadowed((value) -> value.byte_size()) == 5.
"#,
        include_str!("../../../../../std/core/Option.terl"),
    ]);
}

#[test]
fn generic_receiver_callback_does_not_select_an_ordinary_function() {
    check_sources(&[
        r#"
module receiver_callback.
import app.Methods.{Box}.
import app.Others.
pub check(): Bool ->
    let boxed = Box([1, 2]);
    let strings = Box(["value"]);
    boxed.visit((items) -> items.length()) == 2
    and strings.visit((items) -> items.length()) == 1
    and Others.visit(boxed, (_items) -> 0) == 99.
"#,
        r#"
module app.Methods.
pub struct Box[T] { values: List[T] }.
pub constructor Box[T] { (items: List[T]): Box[T] -> Box(values = items) }.
pub (boxed: Box[T]) visit(cb: (List[T]) -> Int): Int -> cb(boxed.values).
"#,
        r#"
module app.Others.
import app.Methods.{Box}.
pub visit[T](boxed: Box[T], cb: (List[T]) -> Int): Int -> 99.
"#,
    ]);
}

#[test]
fn generic_result_callback_equality_retains_variant_layout() {
    check_sources(&[
        r#"
module result_callback_equality.
import std.core.Result.{Result, Ok, Err}.
pub type Problem.
pub trait Mapper[F[_, _]] {
    map[A, B, E](value: F[A, E], transform: (A) -> B): F[B, E].
    map_error[A, E, G](value: F[A, E], transform: (E) -> G): F[A, G].
}.
pub impl Mapper[Result] for Result {
    map(value: Result[A, E], transform: (A) -> B): Result[B, E] ->
        case value { Ok(x) -> Ok(transform(x)); Err(reason) -> Err(reason) }.
    map_error(value: Result[A, E], transform: (E) -> G): Result[A, G] ->
        case value { Ok(x) -> Ok(x); Err(reason) -> Err(transform(reason)) }.
}.
increment(value: Int): Int -> value + 1.
error_text(_problem: Problem): String -> "problem".
pub check(): Bool -> Mapper.map(Ok(1), increment) == Ok(2)
    and Mapper.map(Ok(1), increment) != Ok(3)
    and Mapper.map_error(Err(Problem), error_text) == Err("problem")
    and Mapper.map_error(Err(Problem), error_text) != Err("different").
"#,
        include_str!("../../../../../std/core/Result.terl"),
    ]);
}

#[test]
fn source_constructor_retains_explicit_empty_and_enclosing_type_arguments() {
    let modules = check_sources(&[r#"
module constructor_explicit_empty.
import std.collections.List.
pub type Items[T] = List[T].
pub constructor Items[T] {
    (...values: T): Items[T] -> values
}.
empty[T](): Items[T] -> Items[T]().
pub check(): Bool -> Items[Int]().length() == 0 and Items[String]().length() == 0
    and empty[Int]().length() == 0 and empty[String]().length() == 0.
"#]);
    let collections = modules
        .iter()
        .flat_map(|module| &module.managed_collections)
        .map(|encoded| {
            crate::runtime::native_image::managed::decode_collection_layout(encoded)
                .expect("decode retained collection type")
                .canonical_type()
                .to_string()
        })
        .collect::<Vec<_>>();
    for expected in ["List(Int)", "List(String)"] {
        assert!(
            collections.iter().any(|ty| ty == expected),
            "{expected}: {collections:?}"
        );
    }
}

#[test]
fn source_constructor_explicit_type_arguments_reach_default_arguments() {
    check_source(
        r#"
module constructor_explicit_defaults.
import std.collections.List.
pub type Items[T] = List[T].
pub constructor Items[T] {
    (values: List[T] = []): Items[T] -> values
}.
pub check(): Bool -> Items[Int]().length() == 0 and Items[String]().length() == 0.
"#,
    );
}

#[test]
fn imported_constructor_retains_explicit_provider_type_arguments() {
    check_sources(&[
        r#"
module constructor_explicit_import.
import std.collections.List.
import sample.Provider.{Items, Element}.
pub check(): Bool -> Items[Element]().length() == 0 and Items[String]().length() == 0.
"#,
        r#"
module sample.Provider.
import std.collections.List.
pub struct Element { value: Int }.
pub type Items[T] = List[T].
pub constructor Items[T] {
    (...values: T): Items[T] -> values
}.
"#,
    ]);
}

#[test]
fn transparent_constructor_retains_explicit_empty_payload_type() {
    check_source(
        r#"
module constructor_explicit_alias.
import std.collections.List.
pub type Wrapped[T] = {Atom["wrapped"], values: List[T]}.
empty[T](): Wrapped[T] -> Wrapped[T]([]).
pub check(): Bool ->
    (case Wrapped[Int]([]) { Wrapped(values) -> values.length() == 0 })
    and (case Wrapped[String]([]) { Wrapped(values) -> values.length() == 0 })
    and (case empty[String]() { Wrapped(values) -> values.length() == 0 }).
"#,
    );
}

#[test]
fn struct_constructor_retains_explicit_empty_payload_type() {
    let modules = check_sources(&[r#"
module constructor_explicit_struct.
import std.collections.List.
pub struct Wrapped[T] { values: List[T] }.
empty[T](): Wrapped[T] -> Wrapped[T](values = []).
count[T](value: Wrapped[T]): Int -> value.values.length().
pub check(): Bool -> Wrapped[Int](values = []).values.length() == 0
    and count[Int](empty[Int]()) == 0 and count[String](empty[String]()) == 0.
"#]);
    let layouts = modules
        .iter()
        .flat_map(|module| &module.managed_layouts)
        .map(|encoded| {
            crate::runtime::native_image::managed::decode_aggregate_layout(encoded)
                .expect("decode concrete struct")
                .canonical_type()
                .to_string()
        })
        .collect::<Vec<_>>();
    for expected in [
        "Apply(constructor_explicit_struct.Wrapped;Int)",
        "Apply(constructor_explicit_struct.Wrapped;String)",
    ] {
        assert!(
            layouts.iter().any(|ty| ty == expected),
            "{expected}: {layouts:?}"
        );
    }
}

#[test]
fn source_constructor_executes_its_body() {
    check_source(
        r#"
module constructor_body.
pub type Adjusted = Int.
pub constructor Adjusted {
    (value: Int): Adjusted -> value + 2
}.
pub check(): Bool -> Adjusted(40) == 42.
"#,
    );
}

#[test]
fn private_alias_and_struct_constructors_retain_local_representation() {
    check_source(
        r#"
module private_constructor_representations.
import std.collections.List.
type Hidden = Int.
constructor Hidden { (value: Int): Hidden -> value + 2 }.
type Items[T] = List[T].
constructor Items[T] { (...values: T): Items[T] -> values }.
struct Boxed[T] { values: List[T] }.
pub check(): Bool -> Hidden(40) == 42 and Items[String]().length() == 0
    and Boxed[Int](values = []).values.length() == 0.
"#,
    );
}

#[test]
fn source_constructor_executes_defaults() {
    check_source(
        r#"
module constructor_defaults.
pub type Adjusted = Int.
pub constructor Adjusted {
    (value: Int = 20, extra: Int = 22): Adjusted -> value + extra
}.
pub check(): Bool -> Adjusted() == 42 and Adjusted(30) == 52 and Adjusted(30, 12) == 42.
"#,
    );
}

#[test]
fn source_constructor_retains_generic_body() {
    check_source(
        r#"
module constructor_generic.
pub type Identity[T] = T.
pub constructor Identity[T] {
    (value: T): Identity[T] -> value
}.
pub check(): Bool -> Identity(42) == 42 and Identity(true).
"#,
    );
}

#[test]
fn source_constructor_packs_variadic_arguments() {
    check_source(
        r#"
module constructor_variadic.
import std.collections.List.
pub type Count = Int.
pub constructor Count {
    (...values: Int): Count -> values.length() + 2
}.
pub check(): Bool -> Count() == 2 and Count(10, 20, 30) == 5.
"#,
    );
}

#[test]
fn source_constructor_uses_internal_struct_initializer() {
    check_source(
        r#"
module constructor_struct.
pub struct Value { number: Int }.
pub constructor Value {
    (input: Int): Value -> Value(number = input + 2)
}.
pub check(): Bool -> Value(40).number == 42.
"#,
    );
}

#[test]
fn source_constructor_orders_named_arguments() {
    check_source(
        r#"
module constructor_named.
pub type Difference = Int.
pub constructor Difference {
    (left: Int, right: Int = 2): Difference -> left - right
}.
pub check(): Bool -> Difference(right = 8, left = 50) == 42 and Difference(left = 44) == 42.
"#,
    );
}

fn check_source(source: &str) {
    check_sources(&[source]);
}

#[test]
fn imported_constructor_reaches_private_provider_helpers() {
    check_sources(&[
        r#"
module constructor_import.
import sample.Provider.{Adjusted}.
pub check(): Bool -> Adjusted(extra = 2, value = 40) == 42 and Adjusted(40) == 42.
"#,
        r#"
module sample.Provider.
pub type Adjusted = Int.
offset(value: Int, extra: Int): Int -> value + extra.
pub constructor Adjusted {
    (value: Int, extra: Int = 2): Adjusted -> offset(value, extra)
}.
"#,
    ]);
}

#[test]
fn source_constructor_body_can_suspend() {
    check_source(
        r#"
module constructor_yield.
import std.vm.Process.
pub type Adjusted = Int.
pub constructor Adjusted {
    (value: Int): Adjusted -> let _yielded = Process.yield_now(); value + 2
}.
pub check(): Bool -> Adjusted(40) == 42.
"#,
    );
}

#[test]
fn source_constructor_clauses_use_checked_arity() {
    check_source(
        r#"
module constructor_overload.
pub type Adjusted = Int.
pub constructor Adjusted {
    (value: Int): Adjusted -> value + 2;
    (value: Bool, extra: Int): Adjusted -> if { value -> extra; true -> 0 }
}.
pub check(): Bool -> Adjusted(40) == 42 and Adjusted(true, 42) == 42 and Adjusted(false, 42) == 0.
"#,
    );
}

#[test]
fn source_constructor_retains_empty_generic_vararg_context() {
    check_source(
        r#"
module constructor_empty.
import std.collections.List.
pub type Items[T] = List[T].
pub constructor Items[T] {
    (...values: T): Items[T] -> values
}.
integers(): Items[Int] -> Items().
pub check(): Bool -> integers().length() == 0 and Items(1, 2).length() == 2.
"#,
    );
}

#[test]
fn missing_source_constructor_body_is_not_a_layout_fallback() {
    let syntax = parse_module_as_syntax_output(
        "module missing_constructor. pub type Adjusted = Int. pub constructor Adjusted { (value: Int): Adjusted -> value + 2 }. pub check(): Bool -> Adjusted(40) == 42."
    ).expect("parse constructor");
    let interfaces = checked_in_std_interfaces_for_module(&syntax);
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
    let mut core = lower_syntax_module_output_to_core(&syntax, &resolved);
    core.functions.retain(|function| function.name == "check");
    let error = NativeModule::lower_application(&[&core])
        .expect_err("missing body must fail before linking");
    assert!(
        error.contains("error[native_ir.constructor_body]"),
        "{error}"
    );
}

#[test]
fn generic_constructor_infers_struct_initializer_arguments() {
    check_source(
        r#"
module constructor_struct_arguments.
import std.collections.List.
pub struct Entry { value: Int }.
pub type Items[T] = List[T].
pub constructor Items[T] {
    (...values: T): Items[T] -> values
}.
pub check(): Bool -> Items[Entry](Entry(value = 42)).length() == 1.
"#,
    );
}

#[test]
fn provider_local_intrinsic_calls_use_the_registered_operation() {
    check_source(
        r#"
module std.collections.List.
pub length(values: List[Int]): Int -> 0.
pub check(): Bool -> length([10, 20, 30]) == 3.
"#,
    );
}

pub(super) fn check_sources(sources: &[&str]) -> Vec<NativeModule> {
    let syntaxes = sources
        .iter()
        .map(|source| parse_module_as_syntax_output(source).expect("parse source constructor"))
        .collect::<Vec<_>>();
    let mut interfaces = checked_in_std_interfaces_for_module(&syntaxes[0]);
    for syntax in &syntaxes {
        let interface = syntax_module_output_to_interface(syntax);
        interfaces.insert(interface.module.clone(), interface);
    }
    let mut cores = syntaxes
        .iter()
        .map(|syntax| {
            let resolved = resolve_syntax_module_output_with_interfaces(syntax, &interfaces).module;
            let diagnostics = type_check_syntax_module_output(syntax, &resolved);
            assert!(diagnostics.is_empty(), "{diagnostics:#?}");
            lower_syntax_module_output_to_core(syntax, &resolved)
        })
        .collect::<Vec<_>>();
    let root = cores[0].module.clone();
    super::prune_application_to_function_roots(
        &mut cores,
        &[(root.clone(), "check".to_string(), 0)],
    )
    .expect("retain source constructor dependencies");
    let modules = NativeModule::lower_application(&cores.iter().collect::<Vec<_>>())
        .unwrap_or_else(|error| {
            panic!(
                "lower constructor body: {error}; source CoreIR: {}",
                cores[0].contract_text()
            )
        });
    let object = emit_native_application_object(&root, &modules).expect("emit constructor body");
    let entry = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "check")
        .expect("check export");
    assert_managed_native_object_invocations(
        &root,
        &modules,
        &object,
        &[NativeObjectInvocation {
            export_id: entry.export_id,
            arguments: vec![],
            expected_status: status::OK,
            expected_result: Some(1),
        }],
    );
    modules
}
