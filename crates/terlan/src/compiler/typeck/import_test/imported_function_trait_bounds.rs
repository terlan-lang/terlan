use super::*;
use crate::terlan_hir::{
    resolve_syntax_module_output_with_interfaces, syntax_module_output_to_interface,
};
use crate::terlan_syntax::parse_module_as_syntax_output;

/// An imported receiver method finds a provider-local blanket conformance.
#[test]
fn imported_receiver_method_finds_provider_local_bound_implementation() {
    let provider = parse_module_as_syntax_output(
        r#"
module fixture.Transport.

pub struct Session {
    generation: Int,
}.

pub constructor Session {
    (generation: Int): Session -> Session {generation: generation}
}.

pub trait Portable[T] {
    transfer(value: T): T.
}.

pub impl Portable[T] for T {
    transfer(value: T): T -> value.
}.

pub open(): Session -> Session(1).

pub (session: Session) send[Payload](payload: Payload)[Portable[Payload]]: Payload ->
    payload.
"#,
    )
    .expect("transport fixture");
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );

    let consumer = parse_module_as_syntax_output(
        r#"
module fixture.Consumer.

import fixture.Transport.
import type fixture.Transport.Session.

pub sample(): String -> Transport.open().send("accepted").
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

/// Imported generic conformances refine output-only parameters in declared order.
#[test]
fn imported_generic_provider_refines_outputs_and_explicit_arguments() {
    let contract = parse_module_as_syntax_output(
        r#"
module fixture.Lookup.

pub trait Lookup[Provider, Value, Failure] {
    lookup(provider: Provider): std.core.Result[Value, Failure].
}.
"#,
    )
    .expect("lookup contract");
    let provider = parse_module_as_syntax_output(
        r#"
module fixture.GenericProvider.

import std.core.Result.{Ok}.
import type fixture.Lookup.{Lookup}.

pub type LookupFailure: Int = REJECTED = 0.

pub struct GenericProvider[Value] implements Lookup[GenericProvider[Value], Value, LookupFailure] {
    stored: Value,
}.

pub constructor GenericProvider[Value] {
    (value: Value): GenericProvider[Value] -> GenericProvider {stored: value}
}.

pub (provider: GenericProvider[Value]) lookup(): std.core.Result[Value, LookupFailure] ->
    Ok(provider.stored).
"#,
    )
    .expect("generic provider");
    let consumer_provider = parse_module_as_syntax_output(
        r#"
module fixture.ConsumerProvider.

import fixture.Lookup.
import type fixture.Lookup.{Lookup}.

pub resolve[Provider, Value, Failure](
    provider: Provider,
)[Lookup[Provider, Value, Failure]]: std.core.Result[Value, Failure] ->
    Lookup.lookup(provider).
"#,
    )
    .expect("consumer provider");

    let mut interfaces = HashMap::new();
    for module in [&contract, &provider, &consumer_provider] {
        interfaces.insert(
            module.module_name.clone(),
            syntax_module_output_to_interface(module),
        );
    }

    let consumer = parse_module_as_syntax_output(
        r#"
module fixture.Consumer.

import fixture.ConsumerProvider.{resolve}.
import fixture.GenericProvider.{GenericProvider}.
import type fixture.GenericProvider.{GenericProvider, LookupFailure}.

pub inferred(): std.core.Result[Int, LookupFailure] ->
    resolve(GenericProvider(42)).

pub explicit(): std.core.Result[Int, LookupFailure] ->
    resolve[GenericProvider[Int], Int, LookupFailure](GenericProvider(42)).
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
