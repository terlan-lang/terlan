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
