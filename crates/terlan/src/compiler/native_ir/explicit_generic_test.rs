//! Explicit generic arguments are semantic inputs, including return-only types.

use crate::terlan_hir::{
    checked_in_std_interfaces_for_module, resolve_syntax_module_output_with_interfaces,
};
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output};

use super::native_object_test_support::{
    assert_managed_native_object_invocations, NativeObjectInvocation,
};
use super::{emit_native_application_object, status, NativeModule};

#[test]
fn explicit_generic_return_parameters_keep_distinct_union_layouts() {
    let source = r#"
module explicit_generic_results.
import std.collections.List.
pub type Ok[T] = {Atom["ok"], value: T}.
pub type Err[E] = {Atom["error"], reason: E}.
pub type Result[T, E] = Ok[T] | Err[E].
wrap[T, E](value: T): Result[T, E] -> Ok(value).
nested[T, E](value: T): Result[T, E] -> wrap[T, E](value).
integer_results(): List[Result[Int, Int]] -> [wrap[Int, Int](7)].
string_results(): List[Result[Int, String]] -> [wrap[Int, String](7)].
pub integer_case(): Bool -> case wrap[Int, Int](7) { Ok(value) -> value == 7; Err(_reason) -> false }.
pub string_case(): Bool -> case wrap[Int, String](7) { Ok(value) -> value == 7; Err(_reason) -> false }.
pub nested_case(): Bool -> case nested[Int, String](7) { Ok(value) -> value == 7; Err(_reason) -> false }.
pub list_cases(): Bool -> integer_results().length() == 1 and string_results().length() == 1.
"#;
    check_generic_execution(
        source,
        &["integer_case", "string_case", "nested_case", "list_cases"],
        true,
    );
}

#[test]
fn contextual_return_only_parameters_keep_distinct_union_layouts() {
    check_generic_execution(
        r#"
module contextual_generic_results.
import std.collections.List.
pub type None = Atom["none"].
pub type Some[T] = {Atom["some"], value: T}.
pub type Option[T] = None | Some[T].
missing[Payload](): Option[Payload] -> None.
integers(): List[Option[Int]] -> [missing()].
strings(): List[Option[String]] -> [missing()].
generic[T](value: T): List[Option[T]] -> [missing(), Some(value)].
pub integer_case(): Bool -> integers().length() == 1 and generic(7).length() == 2.
pub string_case(): Bool -> strings().length() == 1 and generic("text").length() == 2.
"#,
        &["integer_case", "string_case"],
        false,
    );
}

#[test]
fn typed_empty_collections_keep_their_witness_through_generic_calls() {
    check_generic_execution(
        r#"
module empty_generic_arguments.
import std.collections.List.
pub type Gen[T] = {Atom["gen"], List[T]}.
elements[T](values: List[T]): Gen[T] -> {Atom["gen"], values}.
empty_binary(): Gen[Binary] -> elements(List.new[Binary]()).
empty_integer(): Gen[Int] -> elements(List.new[Int]()).
empty_binding(): Gen[Binary] -> let values = List.new[Binary](); elements(values).
sample[T](generator: Gen[T]): List[T] -> case generator { {_tag, values} -> values }.
pub check(): Bool -> sample(empty_binary()).length() == 0 and sample(empty_integer()).length() == 0 and sample(empty_binding()).length() == 0.
"#,
        &["check"],
        false,
    );
}

#[test]
fn escaping_generic_callbacks_keep_structural_result_witnesses() {
    check_generic_execution(
        r#"
module structural_generic_callbacks.
import std.collections.List.
map_values[T, U](values: List[T], transform: (T) -> U): List[U] ->
    case values {
        [] -> [];
        [head | tail] -> [transform(head) | map_values(tail, transform)]
    }.
pub check(): Bool ->
    map_values([1, 2], (age) -> {name: "user", age: age})
        == [{name: "user", age: 1}, {name: "user", age: 2}].
"#,
        &["check"],
        false,
    );
}

#[test]
fn inferred_list_of_call_results_retains_its_collection_schema() {
    check_generic_execution(
        r#"
module inferred_generic_results.
import std.collections.List.
pub type None = Atom["none"].
pub type Some[T] = {Atom["some"], value: T}.
pub type Option[T] = None | Some[T].
present(value: Int): Option[Int] -> Some(value).
pub list_case(): Bool ->
    let values = [present(7)];
    values.length() == 1.
"#,
        &["list_case"],
        false,
    );
}

fn check_generic_execution(source: &str, entries: &[&str], explicit: bool) {
    let syntax = parse_module_as_syntax_output(source).expect("parse generic returns");
    let interfaces = checked_in_std_interfaces_for_module(&syntax);
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let contract = core.contract_text();
    if explicit {
        assert!(contract.contains("Call(wrap[Int,Int];"), "{contract}");
        assert!(contract.contains("Call(wrap[Int,String];"), "{contract}");
    }
    let modules =
        NativeModule::lower_application(&[&core]).expect("lower explicit generic returns");
    let object = emit_native_application_object(&core.module, &modules)
        .expect("emit explicit generic returns");
    let invocations = entries
        .iter()
        .map(|name| NativeObjectInvocation {
            export_id: modules
                .iter()
                .flat_map(|module| &module.functions)
                .find(|function| function.name == *name)
                .expect("entry")
                .export_id,
            arguments: vec![],
            expected_status: status::OK,
            expected_result: Some(1),
        })
        .collect::<Vec<_>>();
    assert_managed_native_object_invocations(&core.module, &modules, &object, &invocations);
}
