-module('std_encoding_base64_safe_native').
-export([load/0, metadata/0, operations/0]).
-export([start_worker/1, call_worker/3, dispose_worker/2, stop_worker/1]).
-on_load(load/0).
-export([encode/1]).
-export([decode/1]).
-export([encode_url/1]).
-export([decode_url/1]).

load() ->
    case os:getenv("TERLAN_SAFE_NATIVE_PATH") of
        false -> ok;
        _Path -> ok
    end.

metadata() ->
    #{source_module => <<"std.encoding.Base64">>,
      native_module => <<"std_encoding_base64_safe_native">>,
      scheduler => <<"normal">>,
      operations => operations()}.

operations() ->
    [{<<"encode">>, <<"std.encoding.base64.encode">>, 1},
     {<<"decode">>, <<"std.encoding.base64.decode">>, 1},
     {<<"encode_url">>, <<"std.encoding.base64.encode_url">>, 1},
     {<<"decode_url">>, <<"std.encoding.base64.decode_url">>, 1}].

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

encode(A1) ->
    {error, safe_native_not_loaded_error()}.

decode(A1) ->
    {error, safe_native_not_loaded_error()}.

encode_url(A1) ->
    {error, safe_native_not_loaded_error()}.

decode_url(A1) ->
    {error, safe_native_not_loaded_error()}.

