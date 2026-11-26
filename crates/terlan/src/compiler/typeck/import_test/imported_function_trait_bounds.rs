use super::*;
use crate::terlan_hir::{
    resolve_syntax_module_output_with_interfaces, syntax_module_output_to_interface,
};
use crate::terlan_syntax::parse_module_as_syntax_output;

/// An imported generic function carries its trait evidence transitively.
#[test]
fn selected_imported_function_finds_separate_provider_conformance() {
    let contract = parse_module_as_syntax_output(
        r#"
module fixture.Contract.

pub trait Score[T] {
    score(value: T): Int.
}.

pub score_with[T](value: T)[Score[T]]: Int ->
    Score.score(value).
"#,
    )
    .expect("contract fixture");
    let provider = parse_module_as_syntax_output(
        r#"
module fixture.Provider.

import type fixture.Contract.{Score}.

pub struct Value implements Score[Value] {
    amount: Int,
}.

pub constructor Value {
    (amount: Int): Value -> Value {amount: amount}
}.

pub (value: Value) score(): Int -> value.amount.
"#,
    )
    .expect("provider fixture");

    let mut interfaces = HashMap::new();
    interfaces.insert(
        contract.module_name.clone(),
        syntax_module_output_to_interface(&contract),
    );
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module fixture.Consumer.

import fixture.Contract.{score_with}.
import fixture.Provider.{Value}.

pub sample(): Int -> score_with(Value(7)) + score_with(Value(8)).
"#,
    )
    .expect("consumer fixture");
    let resolved = resolve_syntax_module_output_with_interfaces(&consumer, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&consumer, &resolved);

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {diagnostics:?}"
    );
}

/// A same-leaf trait from another provider cannot satisfy the bound.
#[test]
fn selected_imported_function_rejects_unrelated_provider_conformance() {
    let contract = parse_module_as_syntax_output(
        r#"
module fixture.Contract.

pub trait Score[T] {
    score(value: T): Int.
}.

pub score_with[T](value: T)[Score[T]]: Int ->
    Score.score(value).
"#,
    )
    .expect("contract fixture");
    let unrelated = parse_module_as_syntax_output(
        r#"
module fixture.Unrelated.

pub trait Score[T] {
    score(value: T): Int.
}.

pub struct Value implements Score[Value] {
    amount: Int,
}.

pub constructor Value {
    (amount: Int): Value -> Value {amount: amount}
}.

pub (value: Value) score(): Int -> value.amount.
"#,
    )
    .expect("unrelated fixture");

    let mut interfaces = HashMap::new();
    interfaces.insert(
        contract.module_name.clone(),
        syntax_module_output_to_interface(&contract),
    );
    interfaces.insert(
        unrelated.module_name.clone(),
        syntax_module_output_to_interface(&unrelated),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module fixture.Consumer.

import fixture.Contract.{score_with}.
import fixture.Unrelated.{Value}.

pub sample(): Int -> score_with(Value(7)).
"#,
    )
    .expect("consumer fixture");
    let resolved = resolve_syntax_module_output_with_interfaces(&consumer, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&consumer, &resolved);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("expected trait bound")),
        "expected a canonical trait-bound diagnostic, found: {diagnostics:?}"
    );
}
