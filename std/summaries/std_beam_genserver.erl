-module(std_beam_genserver).

-moduledoc "BEAM GenServer contract.\n\n`std.beam.GenServer` describes the callback shape for BEAM services using\nTerlan traits. Default implementations and concrete runtime lowering remain\ntarget-owned; this module only exposes the typed source contract.".

-export([call/2, cast/2, start/1, stop/1]).

-export_type([call_reply/2, server_ref/4]).

-doc "CallReply contains a new state and reply value.\n\nInput: state type `State` and reply type `Reply`.\nOutput: a structural callback result.\nTransformation: represents a synchronous GenServer reply without leaking the\nbackend tuple shape used by the BEAM implementation.".

-type call_reply(_State, _Reply) :: {state(), reply()}.

-doc "ServerRef represents a running GenServer process.\n\nInput: state, request, reply, and event type parameters.\nOutput: an opaque BEAM-owned process handle.\nTransformation: hides process identity, mailbox mechanics, callback dispatch,\nand supervision details behind a typed receiver-method surface.".

-opaque server_ref(_State, _Request, _Reply, _Event) :: term().

%% trait GenServer.
-doc "Starts a GenServer from an implementation value.\n\nInput: one value whose type implements `GenServer[...]`.\nOutput: `Result[ServerRef[State, Request, Reply, Event], Error]`.\nTransformation: asks the BEAM runtime to initialize typed server state and\nstart a process-backed callback loop. The concrete callback dispatch remains\ntarget-owned.".

-spec start(server()) -> std_core_result:result(server_ref(state(), request(), reply(), event()), std_core_error:error()).

start(Server) ->
    Native.

-doc "Performs a synchronous GenServer request.\n\nInput: one `ServerRef[...]` receiver and one request value.\nOutput: `Result[Reply, Error]`.\nTransformation: sends a typed request to the server process and returns only\nthe callback reply value, keeping state transition messages backend-owned.".

-spec call(server_ref(state(), request(), reply(), event()), request()) -> std_core_result:result(reply(), std_core_error:error()).

call(Server_ref, Request) ->
    Native.

-doc "Queues an asynchronous GenServer event.\n\nInput: one mutable `ServerRef[...]` receiver and one event value.\nOutput: `Unit`.\nTransformation: records an effectful event request while hiding BEAM cast\nmessage shape behind the target runtime.".

-spec cast(server_ref(state(), request(), reply(), event()), event()) -> server_ref(state(), request(), reply(), event()).

cast(Server_ref, Event) ->
    Native.

-doc "Stops a running GenServer.\n\nInput: one mutable `ServerRef[...]` receiver.\nOutput: `Unit`.\nTransformation: asks the BEAM runtime to terminate the server process and\nrelease the target-owned handle.".

-spec stop(server_ref(state(), request(), reply(), event())) -> server_ref(state(), request(), reply(), event()).

stop(Server_ref) ->
    Native.

