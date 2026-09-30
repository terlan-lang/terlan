//! Boolean library calls execute the Terlan provider through ordinary direct AOT.

use crate::terlan_hir::{
    checked_in_std_interfaces_for_module, resolve_syntax_module_output_with_interfaces,
};
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output};

use super::source_constructor_test::checked_provider as provider;
use super::NativeModule;

fn boolean_provider() -> crate::terlan_typeck::CoreModule {
    provider(include_str!("../../../../../std/core/Bool.terl"))
}

/// Deliberately different provider bodies must win over recognized std names.
#[test]
fn boolean_library_names_do_not_replace_source_implementations() {
    super::source_constructor_test::check_sources(&[
        r#"
module bool_provider_authority.
import std.core.Bool.
import std.core.{Int, Float, Unit}.
import std.core.Option.{Some}.
import std.core.Bool.{equal as same, compare as order, to_string as render, from_string as parse}.
apply[T, R](value: T, transform: (T) -> R): R -> transform(value).
pub check(): Bool ->
    same(true, false) and not Bool.equal(true, true)
        and order(false, true) == 42 and Bool.compare(true, false) == 42
        and render(true) == "provider" and Bool.to_string(false) == "provider"
        and parse("not-a-boolean") == Some(true) and Bool.from_string("anything") == Some(true)
        and apply(true, (value) -> value.to_string()) == "provider".
"#,
        r#"
module std.core.Bool.
import std.core.Option.{Some}.
import type std.core.Option.
pub equal(left: Bool, right: Bool): Bool -> left != right.
pub compare(left: Bool, right: Bool): Int -> 42.
pub to_string(value: Bool): String -> "provider".
pub from_string(value: String): Option[Bool] -> Some(true).
"#,
        include_str!("../../../../../std/core/String.terl"),
        include_str!("../../../../../std/core/Option.terl"),
    ]);
}

/// Renaming the shipped provider must not change the library's semantics.
#[test]
fn boolean_library_executes_outside_the_std_namespace() {
    let library = include_str!("../../../../../std/core/Bool.terl").replacen(
        "module std.core.Bool.",
        "module ordinary.Truth.",
        1,
    );
    super::source_constructor_test::check_sources(&[
        r#"
module boolean_portability.
import ordinary.Truth.
import std.core.Option.{None, Some}.
import std.core.Ordering.{Lt}.
pub trait Render[T] { render(value: T): String. }.
pub impl Render[Bool] for Bool {
    render(value: Bool): String -> Truth.to_string(value).
}.
apply[T, R](value: T, transform: (T) -> R): R -> transform(value).
render(value: Bool): String -> "wrong-local-function".
pub check(): Bool ->
    Truth.equal(true, true) and not Truth.equal(false, true)
        and Truth.compare(false, true) == Lt
        and Truth.to_string(false) == "false"
        and Truth.from_string("true") == Some(true)
        and Truth.from_string("True") == None
        and apply(false, (value) -> value.render()) == "false"
        and render(false) == "wrong-local-function".
"#,
        &library,
        include_str!("../../../../../std/core/Option.terl"),
        include_str!("../../../../../std/core/Ordering.terl"),
    ]);
}

#[test]
fn bool_to_string_lowers_composed_scalar_boolean_through_native_ir() {
    let syntax = parse_module_as_syntax_output(
        r#"
module bool_intrinsic_native.

import std.core.Bool.{to_string}.

pub render_composed(): String ->
    to_string("abc".contains("b") and "abc".length() == 3).

pub empty_string_contract(): Bool -> "".is_empty().

pub nonempty_string_contract(): Bool -> not "abc".is_empty().
"#,
    )
    .expect("parse Boolean library source");
    let interfaces = checked_in_std_interfaces_for_module(&syntax);
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);

    let boolean = boolean_provider();
    let ordering = provider(include_str!("../../../../../std/core/Ordering.terl"));
    let option = provider(include_str!("../../../../../std/core/Option.terl"));
    let roots = core
        .functions
        .iter()
        .map(|function| (core.module.clone(), function.name.clone(), function.arity))
        .collect::<Vec<_>>();
    let string = provider(include_str!("../../../../../std/core/String.terl"));
    let mut cores = vec![core, boolean, ordering, option, string];
    super::prune_application_to_function_roots(&mut cores, &roots)
        .expect("retain called library bodies");
    let modules = NativeModule::lower_application(&cores.iter().collect::<Vec<_>>())
        .expect("composed Boolean rendering must lower through direct AOT");

    assert!(modules
        .iter()
        .flat_map(|module| &module.functions)
        .any(|function| function.name == "render_composed"));
    assert!(modules
        .iter()
        .flat_map(|module| &module.functions)
        .any(|function| function.name == "empty_string_contract"));
    assert!(modules
        .iter()
        .flat_map(|module| &module.functions)
        .any(|function| function.name == "nonempty_string_contract"));
}

/// Boolean comparisons and both parsing spellings execute with their exact ABIs.
#[test]
fn bool_library_executes_truth_tables_and_strict_parsing() {
    let syntax = parse_module_as_syntax_output(
        r#"
module bool_primitive_family.
import std.core.Bool.{compare, equal, from_string, to_string}.
import std.core.Ordering.{Eq, Gt, Lt}.
import std.core.Option.{None, Some}.

pub equality(left: Bool, right: Bool): Bool -> equal(left, right).
pub comparison(left: Bool, right: Bool): Bool ->
    compare(left, right) == if {
        left == right -> Eq;
        left -> Gt;
        true -> Lt
    }.
pub parse_true(): Bool ->
    from_string("true") == Some(true) and Bool("true") == Some(true).
pub parse_false(): Bool ->
    from_string("false") == Some(false) and Bool("false") == Some(false).
pub parse_empty(): Bool -> from_string("") == None and Bool("") == None.
pub parse_case(): Bool -> from_string("True") == None and Bool("FALSE") == None.
pub parse_prefix(): Bool -> from_string("trues") == None and Bool("fals") == None.
pub parse_space(): Bool -> from_string(" true") == None and Bool("false ") == None.
pub parse_unicode(): Bool -> from_string("trüe") == None and Bool("真") == None.
pub rendering(value: Bool): Bool ->
    to_string(value) == if { value -> "true"; true -> "false" }.
"#,
    )
    .expect("parse Boolean family source");
    let interfaces = checked_in_std_interfaces_for_module(&syntax);
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let ordering = provider(include_str!("../../../../../std/core/Ordering.terl"));
    let option = provider(include_str!("../../../../../std/core/Option.terl"));
    let boolean = boolean_provider();
    let modules = NativeModule::lower_application(&[&core, &boolean, &ordering, &option])
        .expect("lower Boolean family");
    let object = super::emit_native_application_object("bool-family", &modules)
        .expect("emit Boolean family");
    use super::native_object_test_support::{
        assert_managed_native_object_invocations, NativeObjectInvocation,
    };
    let invocation = |name: &str, arguments: Vec<i64>, expected| {
        let export = modules
            .iter()
            .flat_map(|module| &module.functions)
            .find(|function| function.name == name)
            .expect("Boolean export");
        NativeObjectInvocation {
            export_id: export.export_id,
            arguments,
            expected_status: super::status::OK,
            expected_result: Some(expected),
        }
    };
    let mut invocations = Vec::new();
    for left in [0, 1] {
        for right in [0, 1] {
            invocations.push(invocation(
                "equality",
                vec![left, right],
                i64::from(left == right),
            ));
            invocations.push(invocation("comparison", vec![left, right], 1));
        }
        invocations.push(invocation("rendering", vec![left], 1));
    }
    for name in [
        "parse_true",
        "parse_false",
        "parse_empty",
        "parse_case",
        "parse_prefix",
        "parse_space",
        "parse_unicode",
    ] {
        invocations.push(invocation(name, vec![], 1));
    }
    assert_managed_native_object_invocations("bool-family", &modules, &object, &invocations);
}
