//! Applied union arguments must retain declaration-site trait evidence.

use crate::compiler::native_ir::source_constructor_test::check_sources;

#[test]
fn applied_union_declaration_site_trait_satisfies_generic_bound() {
    check_sources(&[
        r#"
module applied_union.Consumer.
import applied_union.Handler.{Handler}.
import applied_union.Message.{Close, Open}.
import applied_union.MessageHandler.
import std.core.Result.{Ok}.
import type applied_union.MessageHandler.{MessageHandler}.
import type std.core.Result.{Result}.
handle_with[Handler, Message, Output, Failure](
    handler: Handler,
    message: Message,
)[MessageHandler[Handler, Message, Output, Failure]]: Result[Output, Failure] ->
    MessageHandler.handle(handler, message).
pub check(): Bool ->
    handle_with(Handler(10), Open[String]("abc")) == Ok(13)
        and handle_with(Handler(10), Close[String]()) == Ok(10).
"#,
        r#"
module applied_union.MessageHandler.
import type std.core.Result.{Result}.
pub trait MessageHandler[Handler, Message, Output, Failure] {
    handle(handler: Handler, message: Message): Result[Output, Failure].
}.
"#,
        r#"
module applied_union.Message.
pub type OpenTag = Atom["open"].
pub type CloseTag = Atom["close"].
pub type Message[Setup] = {OpenTag, Setup} | CloseTag.
pub constructor Open[Setup] {
    (setup: Setup): Message[Setup] -> {OpenTag, setup}
}.
pub constructor Close[Setup] {
    (): Message[Setup] -> CloseTag
}.
"#,
        r#"
module applied_union.Handler.
import applied_union.MessageHandler.
import applied_union.Message.{Close, Open}.
import std.core.Result.{Ok}.
import type applied_union.Message.{Message}.
import type applied_union.MessageHandler.{MessageHandler}.
import type std.core.Result.{Result}.
pub type Failure: Int = Invalid = 1.
pub struct Handler implements MessageHandler[Handler, Message[String], Int, Failure] {
    count: Int,
}.
pub constructor Handler { (count: Int): Handler -> Handler {count: count} }.
pub (handler: Handler) handle(message: Message[String]): Result[Int, Failure] ->
    case message {
        Open(setup) -> Ok(handler.count + setup.byte_size());
        Close() -> Ok(handler.count)
    }.
"#,
        include_str!("../../../../../../std/core/Result.terl"),
    ]);
}

#[test]
fn result_pattern_preserves_generic_handler_for_followup_dispatch() {
    check_sources(&[
        r#"
module chained_handler.Consumer.
import chained_handler.Handlers.{First, Second}.
import chained_handler.Message.{Close, Open}.
import chained_handler.MessageHandler.
import std.core.Result.{Ok}.
import type chained_handler.MessageHandler.{MessageHandler}.
import type std.core.Result.{Result}.
handle_with[Handler, Message, Output, Failure](
    handler: Handler,
    message: Message,
)[MessageHandler[Handler, Message, Output, Failure]]: Result[Output, Failure] ->
    MessageHandler.handle(handler, message).
pub check(): Bool ->
    (let {
        Ok(first_transition) <- handle_with(First(1), Open[String]("first"));
        Ok(_second_transition) <- handle_with(Second(1), Open[String]("second"))
    } else {
        _ -> false
    };
    (let next_handler = first_transition.current_handler();
    (let Ok(closed_transition) <- handle_with(next_handler, Close[String]()) else {
        _ -> false
    };
    (let final_handler = closed_transition.current_handler();
    final_handler == First(3))))).
"#,
        r#"
module chained_handler.MessageHandler.
import type std.core.Result.{Result}.
pub trait MessageHandler[Handler, Message, Output, Failure] {
    handle(handler: Handler, message: Message): Result[Output, Failure].
}.
"#,
        r#"
module chained_handler.Message.
pub type OpenTag = Atom["open"].
pub type CloseTag = Atom["close"].
pub type Message[Setup] = {OpenTag, Setup} | CloseTag.
pub constructor Open[Setup] {
    (setup: Setup): Message[Setup] -> {OpenTag, setup}
}.
pub constructor Close[Setup] {
    (): Message[Setup] -> CloseTag
}.
"#,
        r#"
module chained_handler.Transition.
pub struct Transition[HandlerState, Emitted] {
    current: HandlerState,
    output: Emitted,
}.
pub constructor Transition[HandlerState, Emitted] {
    (current: HandlerState, output: Emitted): Transition[HandlerState, Emitted] ->
        Transition {current: current, output: output}
}.
pub (transition: Transition[HandlerState, Emitted]) current_handler(): HandlerState ->
    transition.current.
"#,
        r#"
module chained_handler.Handlers.
import chained_handler.MessageHandler.
import chained_handler.Message.{Close, Open}.
import chained_handler.Transition.{Transition}.
import std.core.Result.{Ok}.
import type chained_handler.Message.{Message}.
import type chained_handler.MessageHandler.{MessageHandler}.
import type chained_handler.Transition.{Transition}.
import type std.core.Result.{Result}.
pub struct First implements MessageHandler[First, Message[String], Transition[First, Int], Int] {
    value: Int,
}.
pub struct Second implements MessageHandler[Second, Message[String], Transition[Second, Int], Int] {
    value: Int,
}.
pub constructor First { (value: Int): First -> First {value: value} }.
pub constructor Second { (value: Int): Second -> Second {value: value} }.
pub (handler: First) handle(_message: Message[String]): Result[Transition[First, Int], Int] ->
    Ok(Transition(First(handler.value + 1), handler.value)).
pub (handler: Second) handle(_message: Message[String]): Result[Transition[Second, Int], Int] ->
    Ok(Transition(Second(handler.value + 1), handler.value)).
pub (handler: First) value(): Int -> handler.value.
pub (handler: Second) value(): Int -> handler.value.
"#,
        include_str!("../../../../../../std/core/Result.terl"),
    ]);
}
