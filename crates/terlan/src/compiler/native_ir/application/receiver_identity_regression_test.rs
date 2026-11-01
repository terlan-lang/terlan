//! Imported receiver results and nested unions keep their nominal identity.

use crate::compiler::native_ir::source_constructor_test::check_sources;

#[test]
fn imported_generic_output_keeps_its_receiver_provider() {
    check_sources(&[
        r#"
module receiver_result.Consumer.
import receiver_result.Service.{dispatch}.
pub check(): Bool -> dispatch().emitted_output().target() == 42.
"#,
        r#"
module receiver_result.Dispatch.
pub struct Dispatch[Handler, Output] { handler: Handler, output: Output }.
pub constructor Dispatch[Handler, Output] {
    (handler: Handler, output: Output): Dispatch[Handler, Output] ->
        Dispatch {handler: handler, output: output}
}.
pub (dispatch: Dispatch[Handler, Output]) emitted_output(): Output -> dispatch.output.
"#,
        r#"
module receiver_result.Transition.
import type receiver_result.Target.{Target}.
pub struct Transition[State] { state: State, target_value: Target }.
pub constructor Transition[State] {
    (state: State, target: Target): Transition[State] ->
        Transition {state: state, target_value: target}
}.
pub (transition: Transition[State]) target(): Int -> transition.target_value.value().
"#,
        r#"
module receiver_result.Target.
pub struct Target { amount: Int }.
pub constructor Target { (amount: Int): Target -> Target {amount: amount} }.
pub (target: Target) value(): Int -> target.amount.
"#,
        r#"
module receiver_result.Service.
import receiver_result.Dispatch.{Dispatch}.
import receiver_result.Target.{Target}.
import receiver_result.Transition.{Transition}.
import type receiver_result.Dispatch.{Dispatch}.
import type receiver_result.Transition.{Transition}.
pub dispatch(): Dispatch[Int, Transition[Bool]] ->
    Dispatch(7, Transition(true, Target(42))).
"#,
    ]);
}

#[test]
fn nested_imported_union_keeps_outer_receiver_identity() {
    check_sources(&[
        r#"
module nested_union.Consumer.
import nested_union.Inner.{Value}.
import nested_union.Outer.{Wrapped}.
pub check(): Bool -> Wrapped(Value(42)).payload() == 42.
"#,
        r#"
module nested_union.Inner.
pub type ValueTag = Atom["value"].
pub type EmptyTag = Atom["empty"].
pub type Inner = {ValueTag, Int} | EmptyTag.
pub constructor Value { (value: Int): Inner -> {ValueTag, value} }.
"#,
        r#"
module nested_union.Outer.
import nested_union.Inner.{ValueTag}.
import type nested_union.Inner.{Inner}.
pub type WrappedTag = Atom["wrapped"].
pub type Outer = {WrappedTag, Inner}.
pub constructor Wrapped { (inner: Inner): Outer -> {WrappedTag, inner} }.
pub (outer: Outer) payload(): Int ->
    case outer {
        {WrappedTag, {ValueTag, value}} -> value;
        _ -> 0
    }.
"#,
    ]);
}
