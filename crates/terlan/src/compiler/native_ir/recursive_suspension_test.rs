//! Linked execution and precise-GC coverage for recursive completion frames.

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output};

use super::native_object_test_support::{
    assert_managed_native_object_invocations, NativeObjectInvocation,
};
use super::{emit_native_application_object, status, NativeModule};

fn lower(source: &str) -> Vec<NativeModule> {
    let syntax = parse_module_as_syntax_output(source).expect("parse recursive completion source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    NativeModule::lower_application(&[&core]).expect("lower recursive completion source")
}

#[test]
fn explicitly_typed_conditional_closures_keep_their_local_call_contract() {
    let modules = lower(
        r#"
module conditional_closure.
pub select(choice: Bool, offset: Int): Int ->
    let operation = if {
        choice -> ((value: Int) -> value + offset);
        true -> ((value: Int) -> value - offset)
    };
    operation(10).
"#,
    );
    let export = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "select" && function.public)
        .unwrap();
    assert!(modules
        .iter()
        .flat_map(|module| &module.functions)
        .all(|function| {
            function.source_module == "conditional_closure"
                && function.source_function == "select"
                && function.source_arity == 2
        }));
    let object = emit_native_application_object("conditional_closure", &modules).unwrap();
    let invocations = [(1, 12), (0, 8)]
        .into_iter()
        .map(|(choice, expected)| NativeObjectInvocation {
            export_id: export.export_id,
            arguments: vec![choice, 2],
            expected_status: status::OK,
            expected_result: Some(expected),
        })
        .collect::<Vec<_>>();
    assert_managed_native_object_invocations(
        "typed-conditional-closure",
        &modules,
        &object,
        &invocations,
    );
}

#[test]
fn recursive_completions_preserve_values_and_managed_roots_across_scheduler_entries() {
    let modules = lower(
        r#"
module recursive_completions.

pub struct Node { value: Int }.

read(value: Int, work: Int): Int ->
    if { work == 0 -> value; true -> read(value, work - 1) }.

pub sum(count: Int): Int ->
    if {
        count == 0 -> 0;
        true ->
            let value = read(count, 1);
            let rest = sum(count - 1);
            value + rest
    }.

pub zero_capture(count: Int): Int ->
    if {
        count == 0 -> 0;
        true ->
            let ready = read(1, 1);
            if { ready == 1 -> 1 + zero_capture(count - 1); true -> -1 }
    }.

pub mutual_left(count: Int): Int ->
    if {
        count == 0 -> 0;
        true ->
            let value = read(count, 1);
            value + mutual_right(count - 1)
    }.

mutual_right(count: Int): Int ->
    if {
        count == 0 -> 0;
        true ->
            let value = read(count, 1);
            value + mutual_left(count - 1)
    }.

prepend(node: Node, rest: List[Node]): List[Node] -> [node | rest].

nodes(count: Int): List[Node] ->
    if {
        count == 0 -> [];
        true ->
            let value = read(count, 1);
            let node = Node(value = value);
            let rest = nodes(count - 1);
            prepend(node, rest)
    }.

sum_nodes(values: List[Node], total: Int): Int ->
    case values {
        [] -> total;
        [node | rest] -> sum_nodes(rest, total + node.value)
    }.

pub managed(count: Int): Int -> sum_nodes(nodes(count), 0).
"#,
    );
    let deferred = modules
        .iter()
        .find(|module| module.name == "$terlan.recursive_suspension")
        .expect("recursive calls have scheduler-owned entries");
    assert!(
        deferred.functions.len() <= 5,
        "entry thunks are shared per target"
    );
    assert!(
        modules
            .iter()
            .flat_map(|module| &module.continuations)
            .any(|entry| {
                entry.source_function == "nodes"
                    && entry.params.iter().any(|ty| ty.is_managed_reference())
            }),
        "managed caller captures must be exercised"
    );
    let object = emit_native_application_object("recursive_completions", &modules)
        .expect("bounded recursive completion object");
    let cases = [
        ("sum", 1000, 500500),
        ("zero_capture", 1000, 1000),
        ("mutual_left", 1000, 500500),
        ("managed", 128, 8256),
    ];
    let invocations = cases
        .into_iter()
        .map(|(name, count, expected)| {
            let function = modules
                .iter()
                .flat_map(|module| &module.functions)
                .find(|function| function.public && function.name == name)
                .expect("test export");
            NativeObjectInvocation {
                export_id: function.export_id,
                arguments: vec![count],
                expected_status: status::OK,
                expected_result: Some(expected),
            }
        })
        .collect::<Vec<_>>();
    // This harness parks/restores real managed completion captures and runs a
    // moving owner collection after every scheduler yield.
    assert_managed_native_object_invocations(
        "recursive-completions",
        &modules,
        &object,
        &invocations,
    );
}

#[test]
fn pure_tail_loops_keep_their_existing_lowering() {
    let modules = lower(
        "module recursive_tail_only.\npub sum(count: Int, total: Int): Int -> if { count == 0 -> total; true -> sum(count - 1, total + count) }.\n",
    );
    assert!(modules
        .iter()
        .all(|module| module.name != "$terlan.recursive_suspension"));
    emit_native_application_object("recursive_tail_only", &modules)
        .expect("ordinary tail loop object");
}

#[test]
fn grouped_fallbacks_preserve_closed_union_and_scalar_failures() {
    let modules = lower(
        r#"
module grouped_union_fallback.
pub type Ok[T] = {Atom["ok"], value: T}.
pub type Err[E] = {Atom["error"], reason: E}.
pub type Result[T, E] = Ok[T] | Err[E].
pub type Some[T] = {Atom["some"], value: T}.
pub type None = Atom["none"].
pub type Option[T] = None | Some[T].

read(value: Int, work: Int): Int ->
    if { work == 0 -> value; true -> read(value, work - 1) }.

result(value: Int): Result[Int, Int] ->
    let actual = read(value, 10);
    if { actual < 0 -> Err(actual); true -> Ok(actual) }.

option(value: Int): Option[Int] ->
    if { value == 0 -> None; true -> Some(value) }.

pub evaluate(value: Int, enabled: Bool): Int ->
    let {
        true <- enabled;
        Ok(actual) <- result(value);
        Some(selected) <- option(actual)
    } else {
        Err(reason) -> reason;
        _ -> -99
    };
    selected + 100.
"#,
    );
    let export_id = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.public && function.name == "evaluate")
        .expect("grouped export")
        .export_id;
    let object = emit_native_application_object("grouped_union_fallback", &modules)
        .expect("mixed union fallback object");
    let invocations = [(3, 1, 103), (-7, 1, -7), (0, 1, -99), (3, 0, -99)]
        .into_iter()
        .map(|(value, enabled, expected)| NativeObjectInvocation {
            export_id,
            arguments: vec![value, enabled],
            expected_status: status::OK,
            expected_result: Some(expected),
        })
        .collect::<Vec<_>>();
    assert_managed_native_object_invocations(
        "grouped-union-fallback",
        &modules,
        &object,
        &invocations,
    );
}

#[test]
fn callback_callers_admit_recursive_scheduler_entries() {
    let modules = lower(
        r#"
module recursive_callback.

read(value: Int, work: Int): Int ->
    if { work == 0 -> value; true -> read(value, work - 1) }.

sum(count: Int): Int ->
    if {
        count == 0 -> 0;
        true ->
            let value = read(count, 1);
            value + sum(count - 1)
    }.

operation(choice: Bool, offset: Int): (Int) -> Int ->
    if { choice -> ((value: Int) -> sum(value + offset)); true -> ((value: Int) -> value + offset) }.

invoke(count: Int, choice: Bool, offset: Int): Int ->
    let operation = operation(choice, offset);
    let result = operation(count);
    result + 7.

pub outer(choice: Bool): Int -> invoke(100, choice, 2) + 11.
"#,
    );
    let mut indirect = false;
    for function in modules.iter().flat_map(|module| &module.functions) {
        super::call_composition::walk_native_expr(&function.body, &mut |expr| {
            indirect |= matches!(expr, super::NativeExpr::InvokeClosureThen { .. });
        });
    }
    assert!(indirect, "fixture must retain a real indirect callback");
    let export = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.public && function.name == "outer")
        .expect("outer export")
        .export_id;
    let object = emit_native_application_object("recursive_callback", &modules)
        .expect("recursive callback object");
    assert_managed_native_object_invocations(
        "recursive-callback",
        &modules,
        &object,
        &[
            NativeObjectInvocation {
                export_id: export,
                arguments: vec![1],
                expected_status: status::OK,
                expected_result: Some(5271),
            },
            NativeObjectInvocation {
                export_id: export,
                arguments: vec![0],
                expected_status: status::OK,
                expected_result: Some(120),
            },
        ],
    );
}
