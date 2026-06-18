//! Embedded Erlang runtime helpers emitted beside generated modules.
//!
//! This module owns static Erlang runtime source snippets that are exposed
//! through the backend public API.

/// Returns the embedded Erlang HTML runtime helper module.
///
/// Inputs:
/// - No runtime input.
///
/// Output:
/// - Static Erlang source for the HTML escaping helper module.
///
/// Transformation:
/// - Exposes the checked-in runtime snippet without allocation so callers can
///   write it beside generated Erlang modules when template output needs HTML
///   escaping support.
pub fn emit_html_runtime_to_erlang() -> &'static str {
    TYPER_HTML_RUNTIME
}

const TYPER_HTML_RUNTIME: &str = r#"-module(typer_html).
-export([escape/1]).

escape(Value) when is_binary(Value) ->
    escape_binary(Value);
escape(Value) when is_integer(Value) ->
    integer_to_binary(Value);
escape(true) ->
    <<"true">>;
escape(false) ->
    <<"false">>;
escape(Value) when is_atom(Value) ->
    escape_binary(atom_to_binary(Value, utf8));
escape(Value) ->
    escape_binary(iolist_to_binary(io_lib:format("~p", [Value]))).

escape_binary(Bin) ->
    escape_binary(Bin, []).

escape_binary(<<>>, Acc) ->
    lists:reverse(Acc);
escape_binary(<<"&", Rest/binary>>, Acc) ->
    escape_binary(Rest, [<<"&amp;">> | Acc]);
escape_binary(<<"<", Rest/binary>>, Acc) ->
    escape_binary(Rest, [<<"&lt;">> | Acc]);
escape_binary(<<">", Rest/binary>>, Acc) ->
    escape_binary(Rest, [<<"&gt;">> | Acc]);
escape_binary(<<"\"", Rest/binary>>, Acc) ->
    escape_binary(Rest, [<<"&quot;">> | Acc]);
escape_binary(<<"'", Rest/binary>>, Acc) ->
    escape_binary(Rest, [<<"&#39;">> | Acc]);
escape_binary(<<Char, Rest/binary>>, Acc) ->
    escape_binary(Rest, [<<Char>> | Acc]).
"#;
