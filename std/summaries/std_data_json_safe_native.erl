-module('std_data_json_safe_native').
-export([load/0, metadata/0, operations/0]).
-export([start_worker/1, call_worker/3, dispose_worker/2, stop_worker/1]).
-on_load(load/0).
-export([parse/1]).
-export([stringify/1]).
-export([get/2]).
-export([as_string/1]).
-export([as_int/1]).
-export([as_float/1]).
-export([as_bool/1]).
-export([is_null/1]).

load() ->
    case os:getenv("TERLAN_SAFE_NATIVE_PATH") of
        false -> ok;
        _Path -> ok
    end.

metadata() ->
    #{source_module => <<"std.data.Json">>,
      native_module => <<"std_data_json_safe_native">>,
      scheduler => <<"normal">>,
      operations => operations()}.

operations() ->
    [{<<"parse">>, <<"std.data.json.parse">>, 1},
     {<<"stringify">>, <<"std.data.json.stringify">>, 1},
     {<<"get">>, <<"std.data.json.get">>, 2},
     {<<"as_string">>, <<"std.data.json.as_string">>, 1},
     {<<"as_int">>, <<"std.data.json.as_int">>, 1},
     {<<"as_float">>, <<"std.data.json.as_float">>, 1},
     {<<"as_bool">>, <<"std.data.json.as_bool">>, 1},
     {<<"is_null">>, <<"std.data.json.is_null">>, 1}].

start_worker(_Options) ->
    {error, safe_native_not_loaded_error()}.

call_worker(RequestId, Operation, Args) when is_integer(RequestId), is_list(Args) ->
    _ = Operation,
    {safe_native_reply, RequestId, {error, safe_native_not_loaded_error()}, 0}.

dispose_worker(RequestId, _Handle) when is_integer(RequestId) ->
    {safe_native_reply, RequestId, {error, safe_native_not_loaded_error()}, 0}.

stop_worker(_Bridge) ->
    ok.

safe_native_not_loaded_error() ->
    #{code => <<"safe_native.not_loaded">>,
      message => <<"SafeNative library is not loaded.">>,
      offset => 0}.

parse(A1) ->
    {error, safe_native_not_loaded_error()}.

stringify(A1) ->
    {error, safe_native_not_loaded_error()}.

get(A1, A2) ->
    {error, safe_native_not_loaded_error()}.

as_string(A1) ->
    {error, safe_native_not_loaded_error()}.

as_int(A1) ->
    {error, safe_native_not_loaded_error()}.

as_float(A1) ->
    {error, safe_native_not_loaded_error()}.

as_bool(A1) ->
    {error, safe_native_not_loaded_error()}.

is_null(A1) ->
    {error, safe_native_not_loaded_error()}.

