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

/// Returns the embedded Erlang SQL runtime boundary module.
///
/// Inputs:
/// - No runtime input.
///
/// Output:
/// - Static Erlang source for the SQL wrapper boundary module.
///
/// Transformation:
/// - Exposes the checked-in BEAM boundary that compiles beside generated query
///   wrappers. The module delegates live execution to the private `terlc
///   __sql-runtime` helper and converts projected rows back into normal Terlan
///   record tuples.
pub fn emit_sql_runtime_to_erlang() -> &'static str {
    TERLAN_SQL_RUNTIME
}

/// Returns the embedded Erlang SafeNative vector boundary module.
///
/// Inputs:
/// - No runtime input.
///
/// Output:
/// - Static Erlang source for the `std.native.collections.Vector` bridge
///   module.
///
/// Transformation:
/// - Exposes a checked-in bridge module that compiles beside generated modules
///   and returns opaque vector handles. The generated BEAM module delegates to
///   a compiler-owned Rust SafeNative helper. A non-ETS fallback exists only
///   behind `TERLAN_NATIVE_VECTOR_RUNTIME_ALLOW_BEAM_FALLBACK=1` for tests that
///   deliberately exercise helper-missing behavior.
pub fn emit_native_vector_runtime_to_erlang() -> &'static str {
    TERLAN_NATIVE_VECTOR_RUNTIME
}

/// Returns the embedded Erlang NativeBridge boundary module.
///
/// Inputs:
/// - No runtime input.
///
/// Output:
/// - Static Erlang source for the `std.beam.NativeBridge` helper module.
///
/// Transformation:
/// - Exposes a small OTP reference boundary whose functions are mirrored by
///   the Rust VM host/native registry. The helper intentionally returns plain
///   bridge values; source-level `Result` wrapping stays in compiler-emitted
///   module code so the Rust VM host function does not need atom-table access.
pub fn emit_native_bridge_runtime_to_erlang() -> &'static str {
    TERLAN_NATIVE_BRIDGE_RUNTIME
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

const TERLAN_NATIVE_BRIDGE_RUNTIME: &str = r#"-module(terlan_native_bridge_runtime).
-export([start/1, call/2, dispose/1, stop/1]).

start(Resource) ->
    Resource.

call(_Bridge, Command) ->
    Command.

dispose(Bridge) ->
    Bridge.

stop(Bridge) ->
    Bridge.
"#;

const TERLAN_SQL_RUNTIME: &str = r#"-module(terlan_sql_runtime).
-export([query_one/5, query/5, execute/5]).

query_one(Sql, Params, RowType, Projection, _ResultType) ->
    case run_helper(<<"query_one">>, Sql, Params, Projection) of
        {ok_none} -> {ok, none};
        {ok_one, Values} -> {ok, {some, row_record(RowType, Values)}};
        {err, Message} -> {error, {sql_error, Message}}
    end.

query(Sql, Params, RowType, Projection, _ResultType) ->
    case run_helper(<<"query">>, Sql, Params, Projection) of
        {ok_rows, Rows} -> {ok, [row_record(RowType, Values) || Values <- Rows]};
        {err, Message} -> {error, {sql_error, Message}}
    end.

execute(Sql, Params, _RowType, Projection, _ResultType) ->
    case run_helper(<<"execute">>, Sql, Params, Projection) of
        {ok_int, Value} -> {ok, Value};
        {err, Message} -> {error, {sql_error, Message}}
    end.

run_helper(Operation, Sql, Params, Projection) ->
    Helper = helper_path(),
    Args = [
        "__sql-runtime",
        binary_to_list(Operation),
        binary_to_list(base64:encode(Sql)),
        binary_to_list(base64:encode(params_json(Params))),
        binary_to_list(base64:encode(projection_text(Projection)))
    ],
    Port = open_port({spawn_executable, Helper}, [binary, exit_status, stderr_to_stdout, {args, Args}]),
    parse_helper_output(collect_port(Port, [])).

helper_path() ->
    case os:getenv("TERLAN_SQL_RUNTIME_HELPER") of
        false -> "terlc";
        Path -> Path
    end.

collect_port(Port, Acc) ->
    receive
        {Port, {data, Bytes}} ->
            collect_port(Port, [Bytes | Acc]);
        {Port, {exit_status, 0}} ->
            iolist_to_binary(lists:reverse(Acc));
        {Port, {exit_status, Status}} ->
            error({terlan_sql_runtime_helper_failed, Status, iolist_to_binary(lists:reverse(Acc))})
    end.

parse_helper_output(Output) ->
    Lines = non_empty_lines(trim_trailing_newlines(Output)),
    case Lines of
        [<<"ok_none">>] ->
            {ok_none};
        [<<"ok_int">>, Value] ->
            {ok_int, binary_to_integer(Value)};
        [<<"ok_one">>, Row] ->
            {ok_one, parse_row(Row)};
        [<<"ok_rows">> | Rows] ->
            {ok_rows, [parse_row(Row) || Row <- Rows]};
        [<<"err">>, Encoded] ->
            {err, base64:decode(Encoded)};
        _ ->
            {err, <<"invalid SQL runtime helper response">>}
    end.

non_empty_lines(<<>>) ->
    [];
non_empty_lines(Output) ->
    [Line || Line <- binary:split(Output, <<"\n">>, [global]), Line =/= <<>>].

trim_trailing_newlines(<<>>) ->
    <<>>;
trim_trailing_newlines(Bin) ->
    Size = byte_size(Bin),
    case binary:at(Bin, Size - 1) of
        $\n -> trim_trailing_newlines(binary:part(Bin, 0, Size - 1));
        $\r -> trim_trailing_newlines(binary:part(Bin, 0, Size - 1));
        _ -> Bin
    end.

parse_row(Line) ->
    [parse_value(Field) || Field <- binary:split(Line, <<"\t">>, [global])].

parse_value(<<"i:", Rest/binary>>) ->
    binary_to_integer(Rest);
parse_value(<<"b:true">>) ->
    true;
parse_value(<<"b:false">>) ->
    false;
parse_value(<<"s:", Rest/binary>>) ->
    base64:decode(Rest);
parse_value(<<"j:", Rest/binary>>) ->
    base64:decode(Rest);
parse_value(Other) ->
    {sql_runtime_unknown_value, Other}.

row_record(RowType, Values) ->
    list_to_tuple([record_atom(RowType) | Values]).

record_atom(RowType) when is_binary(RowType) ->
    list_to_atom(string:lowercase(binary_to_list(RowType)));
record_atom(RowType) when is_list(RowType) ->
    list_to_atom(string:lowercase(RowType));
record_atom(RowType) when is_atom(RowType) ->
    RowType.

projection_text(Projection) ->
    iolist_to_binary(join_binary(Projection, <<"\n">>)).

params_json(Params) ->
    iolist_to_binary([$[, join_binary([json_value(Param) || Param <- Params], <<",">>), $]]).

json_value(Value) when is_binary(Value) ->
    json_string(Value);
json_value(Value) when is_integer(Value) ->
    integer_to_binary(Value);
json_value(Value) when is_float(Value) ->
    float_to_binary(Value, [short]);
json_value(true) ->
    <<"true">>;
json_value(false) ->
    <<"false">>;
json_value(none) ->
    <<"null">>;
json_value(null) ->
    <<"null">>;
json_value(undefined) ->
    <<"null">>;
json_value(Value) when is_list(Value) ->
    iolist_to_binary([$[, join_binary([json_value(Item) || Item <- Value], <<",">>), $]]);
json_value(Value) when is_atom(Value) ->
    json_string(atom_to_binary(Value, utf8));
json_value(Value) ->
    json_string(iolist_to_binary(io_lib:format("~p", [Value]))).

json_string(Value) ->
    Escaped = json_escape(Value, []),
    iolist_to_binary([$", lists:reverse(Escaped), $"]).

json_escape(<<>>, Acc) ->
    Acc;
json_escape(<<"\\", Rest/binary>>, Acc) ->
    json_escape(Rest, [<<"\\\\">> | Acc]);
json_escape(<<"\"", Rest/binary>>, Acc) ->
    json_escape(Rest, [<<"\\\"">> | Acc]);
json_escape(<<"\n", Rest/binary>>, Acc) ->
    json_escape(Rest, [<<"\\n">> | Acc]);
json_escape(<<"\r", Rest/binary>>, Acc) ->
    json_escape(Rest, [<<"\\r">> | Acc]);
json_escape(<<"\t", Rest/binary>>, Acc) ->
    json_escape(Rest, [<<"\\t">> | Acc]);
json_escape(<<Char, Rest/binary>>, Acc) when Char < 16 ->
    json_escape(Rest, [io_lib:format("\\u000~.16B", [Char]) | Acc]);
json_escape(<<Char, Rest/binary>>, Acc) when Char < 32 ->
    json_escape(Rest, [io_lib:format("\\u00~.16B", [Char]) | Acc]);
json_escape(<<Char, Rest/binary>>, Acc) ->
    json_escape(Rest, [<<Char>> | Acc]).

join_binary([], _Separator) ->
    [];
join_binary([Item], _Separator) ->
    [Item];
join_binary([Item | Rest], Separator) ->
    [Item, Separator | join_binary(Rest, Separator)].
"#;

const TERLAN_NATIVE_VECTOR_RUNTIME: &str = r#"-module(std_native_collections_vector_safe_native).
-export([new/0, from_list/1, length/1, get_at/2, set_at/3, swap/3, push/2, to_list/1]).
-export_type([vector/1]).

-type vector(_T) :: {terlan_native_vector, non_neg_integer(), non_neg_integer()}.

-define(SERVER, terlan_native_vector_runtime_server).

new() ->
    case helper_call(<<"new">>) of
        {ok_handle, Id, Generation} -> {terlan_native_vector, Id, Generation};
        Error -> Error
    end.

from_list(Values) when is_list(Values) ->
    Command = iolist_to_binary([<<"from_list ">>, encode_terms(Values)]),
    case helper_call(Command) of
        {ok_handle, Id, Generation} -> {terlan_native_vector, Id, Generation};
        Error -> Error
    end;
from_list(Value) ->
    {error, {invalid_vector_source, Value}}.

length(Vector) ->
    with_vector_handle(Vector, fun(Id, Generation) ->
        helper_call(iolist_to_binary([<<"length ">>, integer_to_binary(Id), <<" ">>, integer_to_binary(Generation)]))
    end).

get_at(Vector, Index) when is_integer(Index), Index >= 0 ->
    with_vector_handle(Vector, fun(Id, Generation) ->
        case helper_call(iolist_to_binary([<<"get_at ">>, integer_to_binary(Id), <<" ">>, integer_to_binary(Generation), <<" ">>, integer_to_binary(Index)])) of
            {ok_term, Encoded} ->
                case decode_term_result(Encoded) of
                    {ok, Value} -> Value;
                    Error -> Error
                end;
            Error -> Error
        end
    end);
get_at(_Vector, Index) ->
    {error, {invalid_vector_index, Index}}.

set_at(Vector, Index, Value) when is_integer(Index), Index >= 0 ->
    with_vector_handle(Vector, fun(Id, Generation) ->
        case helper_call(iolist_to_binary([<<"set_at ">>, integer_to_binary(Id), <<" ">>, integer_to_binary(Generation), <<" ">>, integer_to_binary(Index), <<" ">>, encode_term(Value)])) of
            {ok_handle, NextId, NextGeneration} -> {terlan_native_vector, NextId, NextGeneration};
            Error -> Error
        end
    end);
set_at(_Vector, Index, _Value) ->
    {error, {invalid_vector_index, Index}}.

swap(Vector, Left, Right) when is_integer(Left), Left >= 0, is_integer(Right), Right >= 0 ->
    with_vector_handle(Vector, fun(Id, Generation) ->
        case helper_call(iolist_to_binary([<<"swap ">>, integer_to_binary(Id), <<" ">>, integer_to_binary(Generation), <<" ">>, integer_to_binary(Left), <<" ">>, integer_to_binary(Right)])) of
            {ok_handle, NextId, NextGeneration} -> {terlan_native_vector, NextId, NextGeneration};
            Error -> Error
        end
    end);
swap(_Vector, Left, Right) ->
    {error, {invalid_vector_indexes, Left, Right}}.

push(Vector, Value) ->
    with_vector_handle(Vector, fun(Id, Generation) ->
        case helper_call(iolist_to_binary([<<"push ">>, integer_to_binary(Id), <<" ">>, integer_to_binary(Generation), <<" ">>, encode_term(Value)])) of
            {ok_handle, NextId, NextGeneration} -> {terlan_native_vector, NextId, NextGeneration};
            Error -> Error
        end
    end).

to_list(Vector) ->
    with_vector_handle(Vector, fun(Id, Generation) ->
        case helper_call(iolist_to_binary([<<"to_list ">>, integer_to_binary(Id), <<" ">>, integer_to_binary(Generation)])) of
            {ok_terms, EncodedValues} -> decode_terms(EncodedValues);
            Error -> Error
        end
    end).

with_vector_handle({terlan_native_vector, Id, Generation}, Fun)
        when is_integer(Id), Id >= 0, is_integer(Generation), Generation >= 0 ->
    Fun(Id, Generation);
with_vector_handle(Value, _Fun) ->
    {error, {invalid_native_vector, Value}}.

helper_call(Command) ->
    Server = ensure_server(),
    Ref = make_ref(),
    Server ! {native_vector_call, self(), Ref, Command},
    receive
        {native_vector_reply, Ref, Reply} -> parse_reply(Reply)
    after 10000 ->
        {error, native_vector_runtime_timeout}
    end.

ensure_server() ->
    case whereis(?SERVER) of
        undefined ->
            Pid = spawn_link(fun() -> server_loop(open_runtime()) end),
            case catch register(?SERVER, Pid) of
                true -> Pid;
                _ ->
                    case whereis(?SERVER) of
                        undefined -> Pid;
                        Existing -> Existing
                    end
            end;
        Pid ->
            Pid
    end.

open_runtime() ->
    case helper_path() of
        false -> fallback_or_error(<<"native vector runtime helper `terlc` was not found">>);
        Helper ->
            try {port, open_port({spawn_executable, Helper}, [binary, {line, 1048576}, {args, [<<"__native-vector-runtime">>]}, exit_status])}
            catch _:_ -> fallback_or_error(<<"native vector runtime helper could not be started">>)
            end
    end.

fallback_or_error(Message) ->
    case allow_beam_fallback() of
        true -> {fallback, #{next => 1, vectors => #{}}};
        false -> {error, Message}
    end.

allow_beam_fallback() ->
    os:getenv("TERLAN_NATIVE_VECTOR_RUNTIME_ALLOW_BEAM_FALLBACK") =:= "1".

helper_path() ->
    case os:getenv("TERLAN_NATIVE_VECTOR_RUNTIME_HELPER") of
        false -> os:find_executable("terlc");
        Path -> resolve_helper_path(Path)
    end.

resolve_helper_path(Path) ->
    case filename:pathtype(Path) of
        relative -> os:find_executable(Path);
        _ -> Path
    end.

server_loop({port, Port} = Runtime) ->
    receive
        {native_vector_call, From, Ref, Command} ->
            port_command(Port, [Command, <<"\n">>]),
            receive
                {Port, {data, {eol, Line}}} ->
                    handle_port_line(Runtime, From, Ref, Command, normalize_line(Line));
                {Port, {data, {noeol, Line}}} ->
                    handle_port_line(Runtime, From, Ref, Command, normalize_line(Line));
                {Port, {exit_status, Status}} ->
                    From ! {native_vector_reply, Ref, <<"err native_vector_runtime_exit ", (base64:encode(integer_to_binary(Status)))/binary>>},
                    server_loop(open_runtime())
            after 10000 ->
                From ! {native_vector_reply, Ref, <<"err native_vector_runtime_timeout dGltZW91dA==">>},
                server_loop(Runtime)
            end
    end;
server_loop({fallback, State}) ->
    receive
        {native_vector_call, From, Ref, Command} ->
            {Reply, NextState} = fallback_call(Command, State),
            From ! {native_vector_reply, Ref, Reply},
            server_loop({fallback, NextState})
    end;
server_loop({error, Message}) ->
    receive
        {native_vector_call, From, Ref, _Command} ->
            From ! {native_vector_reply, Ref, <<"err native_vector_runtime_unavailable ", (base64:encode(Message))/binary>>},
            server_loop({error, Message})
    end.

normalize_line(Line) when is_binary(Line) -> Line;
normalize_line(Line) when is_list(Line) -> list_to_binary(Line).

handle_port_line(Runtime, From, Ref, Command, Line) ->
    case is_helper_reply(Line) of
        true ->
            From ! {native_vector_reply, Ref, Line},
            server_loop(Runtime);
        false ->
            case Runtime of
                {port, Port} -> catch port_close(Port);
                _ -> ok
            end,
            case allow_beam_fallback() of
                true ->
                    EmptyState = #{next => 1, vectors => #{}},
                    {Reply, NextState} = fallback_call(Command, EmptyState),
                    From ! {native_vector_reply, Ref, Reply},
                    server_loop({fallback, NextState});
                false ->
                    From ! {native_vector_reply, Ref, <<"err native_vector_runtime_protocol ", (base64:encode(Line))/binary>>},
                    server_loop({error, <<"native vector runtime helper returned invalid protocol">>})
            end
    end.

is_helper_reply(<<"ok_", _/binary>>) -> true;
is_helper_reply(<<"err ", _/binary>>) -> true;
is_helper_reply(_Line) -> false.

parse_reply(Reply) ->
    case binary:split(Reply, <<" ">>, [global]) of
        [<<"ok_unit">>] -> ok;
        [<<"ok_int">>, Value] -> parse_integer_reply(Value);
        [<<"ok_term">>, Encoded] -> {ok_term, Encoded};
        [<<"ok_terms">>, Encoded] -> {ok_terms, split_encoded_terms(Encoded)};
        [<<"ok_terms">>] -> {ok_terms, []};
        [<<"ok_handle">>, Id, Generation] -> parse_handle_reply(Id, Generation);
        [<<"err">>, Code, Message] -> {error, {binary_to_atom(Code), decode_message(Message)}};
        _ -> {error, {native_vector_runtime_protocol, Reply}}
    end.

parse_integer_reply(Value) ->
    case parse_integer(Value) of
        {ok, Parsed} -> Parsed;
        Error -> Error
    end.

parse_handle_reply(Id, Generation) ->
    case {parse_integer(Id), parse_integer(Generation)} of
        {{ok, ParsedId}, {ok, ParsedGeneration}} -> {ok_handle, ParsedId, ParsedGeneration};
        {Error = {error, _Reason}, _} -> Error;
        {_, Error = {error, _Reason}} -> Error
    end.

parse_integer(Value) ->
    try {ok, binary_to_integer(Value)}
    catch _:_ -> {error, {native_vector_invalid_integer, Value}}
    end.

binary_to_atom(Value) ->
    erlang:binary_to_atom(Value, utf8).

decode_message(Value) ->
    case base64:decode(Value) of
        Message when is_binary(Message) -> Message;
        _ -> Value
    end.

encode_terms(Values) ->
    join_encoded([encode_term(Value) || Value <- Values]).

encode_term(Value) ->
    base64:encode(term_to_binary(Value)).

decode_term(Encoded) ->
    case decode_term_result(Encoded) of
        {ok, Value} -> Value;
        Error -> Error
    end.

decode_term_result(Encoded) ->
    try {ok, binary_to_term(base64:decode(Encoded))}
    catch _:_ -> {error, {native_vector_invalid_term, Encoded}}
    end.

decode_terms(EncodedValues) ->
    decode_terms(EncodedValues, []).

decode_terms([], Acc) ->
    lists:reverse(Acc);
decode_terms([Encoded | Rest], Acc) ->
    case decode_term_result(Encoded) of
        {ok, Value} -> decode_terms(Rest, [Value | Acc]);
        Error -> Error
    end.

join_encoded([]) ->
    <<>>;
join_encoded([Head | Tail]) ->
    lists:foldl(fun(Value, Acc) -> <<Acc/binary, ",", Value/binary>> end, Head, Tail).

split_encoded_terms(<<>>) ->
    [];
split_encoded_terms(Encoded) ->
    binary:split(Encoded, <<",">>, [global]).

fallback_call(<<"new">>, State) ->
    fallback_from_values([], State);
fallback_call(<<"from_list ", Encoded/binary>>, State) ->
    fallback_from_values([decode_term(Value) || Value <- split_encoded_terms(Encoded)], State);
fallback_call(Command, State) ->
    case binary:split(Command, <<" ">>, [global]) of
        [<<"length">>, Id, _Generation] ->
            Values = fallback_values(binary_to_integer(Id), State),
            {<<"ok_int ", (integer_to_binary(erlang:length(Values)))/binary>>, State};
        [<<"get_at">>, Id, _Generation, Index] ->
            {fallback_get(binary_to_integer(Id), binary_to_integer(Index), State), State};
        [<<"set_at">>, Id, Generation, Index, Encoded] ->
            fallback_update(binary_to_integer(Id), binary_to_integer(Generation), State, fun(Values) ->
                replace_zero_based(Values, binary_to_integer(Index), decode_term(Encoded))
            end);
        [<<"swap">>, Id, Generation, Left, Right] ->
            fallback_update(binary_to_integer(Id), binary_to_integer(Generation), State, fun(Values) ->
                swap_zero_based(Values, binary_to_integer(Left), binary_to_integer(Right))
            end);
        [<<"push">>, Id, Generation, Encoded] ->
            fallback_update(binary_to_integer(Id), binary_to_integer(Generation), State, fun(Values) ->
                {ok, Values ++ [decode_term(Encoded)]}
            end);
        [<<"to_list">>, Id, _Generation] ->
            Values = fallback_values(binary_to_integer(Id), State),
            {<<"ok_terms ", (encode_terms(Values))/binary>>, State};
        _ ->
            {<<"err native_vector_unknown_command dW5rbm93biBjb21tYW5k">>, State}
    end.

fallback_from_values(Values, #{next := Next, vectors := Vectors} = State) ->
    Updated = State#{next => Next + 1, vectors => maps:put(Next, Values, Vectors)},
    {<<"ok_handle ", (integer_to_binary(Next))/binary, " 1">>, Updated}.

fallback_values(Id, #{vectors := Vectors}) ->
    maps:get(Id, Vectors, []).

fallback_get(Id, Index, State) ->
    Values = fallback_values(Id, State),
    case safe_nth(Values, Index + 1) of
        {ok, Value} -> <<"ok_term ", (encode_term(Value))/binary>>;
        error -> <<"err vector.index_out_of_bounds aW5kZXggb3V0IG9mIGJvdW5kcw==">>
    end.

fallback_update(Id, Generation, #{vectors := Vectors} = State, Fun) ->
    Values = maps:get(Id, Vectors, []),
    case Fun(Values) of
        {ok, UpdatedValues} ->
            {<<"ok_handle ", (integer_to_binary(Id))/binary, " ", (integer_to_binary(Generation))/binary>>, State#{vectors => maps:put(Id, UpdatedValues, Vectors)}};
        {error, _Reason} ->
            {<<"err vector.index_out_of_bounds aW5kZXggb3V0IG9mIGJvdW5kcw==">>, State}
    end.

replace_zero_based(Values, Index, Value) ->
    replace_one_based(Values, Index + 1, Value, []).

replace_one_based([], _Index, _Value, _Acc) ->
    {error, index_out_of_bounds};
replace_one_based([_Head | Tail], 1, Value, Acc) ->
    {ok, lists:reverse(Acc, [Value | Tail])};
replace_one_based([Head | Tail], Index, Value, Acc) when Index > 1 ->
    replace_one_based(Tail, Index - 1, Value, [Head | Acc]).

swap_zero_based(Values, Left, Right) ->
    case {safe_nth(Values, Left + 1), safe_nth(Values, Right + 1)} of
        {{ok, LeftValue}, {ok, RightValue}} ->
            case replace_zero_based(Values, Left, RightValue) of
                {ok, LeftUpdated} -> replace_zero_based(LeftUpdated, Right, LeftValue);
                Error -> Error
            end;
        _ ->
            {error, index_out_of_bounds}
    end.

safe_nth(Values, Index) when Index > 0 ->
    safe_nth_one_based(Values, Index);
safe_nth(_Values, _Index) ->
    error.

safe_nth_one_based([], _Index) ->
    error;
safe_nth_one_based([Head | _Tail], 1) ->
    {ok, Head};
safe_nth_one_based([_Head | Tail], Index) ->
    safe_nth_one_based(Tail, Index - 1).
"#;
