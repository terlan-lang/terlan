-module(std_beam_agent).

-moduledoc "BEAM Agent contract.\n\n`std.beam.Agent` models a supervised process that owns one typed state value.\nIt is a BEAM convenience wrapper built on the same ordinary type and method\nrules as the rest of Terlan; it is not a portable mutable reference.".

-export([cast/2, get/1, get_and_update/2, start/1, stop/1, update/2]).

-export_type([agent/1]).

-doc "Agent represents a supervised typed state process.\n\nInput: type parameter `T`.\nOutput: an opaque BEAM-owned process handle.\nTransformation: hides process identity, mailbox mechanics, and supervision\ndetails behind a typed receiver-method surface.".

-opaque agent(_T) :: term().

-doc "Starts an agent with an initial value.\n\nInput: one initial state value of type `T`.\nOutput: `Result[Agent[T], Error]`.\nTransformation: asks the BEAM runtime to start a supervised state process and\nnormalizes startup failure into the standard typed error channel.".

-spec start(T) -> std_core_result:result(agent(T), std_core_error:error()).

start(Initial) ->
    Native.

-doc "Reads the current state.\n\nInput: one `Agent[T]` receiver.\nOutput: the current state value.\nTransformation: performs a typed synchronous state read without exposing a\nprocess message or reply tuple in Terlan source.".

-spec get(agent(T)) -> T.

get(Agent) ->
    Native.

-doc "Replaces state by applying a writer function.\n\nInput: one mutable `Agent[T]` receiver and one writer from `T` to `T`.\nOutput: `Unit`.\nTransformation: updates the agent state through receiver mutability while\npreserving BEAM process ownership behind the target runtime.".

-spec update(agent(T), fun((T) -> T)) -> agent(T).

update(Agent, Writer) ->
    Native.

-doc "Updates state and returns a derived value.\n\nInput: one `Agent[T]` receiver and one writer from `T` to a named\n`{state, value}` pair.\nOutput: the derived value.\nTransformation: performs a single state transition and read through the\nBEAM-owned process. The receiver handle remains stable; the target runtime\nowns the state transition.".

-spec get_and_update(agent(T), fun((T) -> {T, U})) -> U.

get_and_update(Agent, Writer) ->
    Native.

-doc "Queues an asynchronous state update.\n\nInput: one mutable `Agent[T]` receiver and one writer from `T` to `T`.\nOutput: `Unit`.\nTransformation: records an effectful state update request without exposing\nBEAM cast messages in Terlan source.".

-spec cast(agent(T), fun((T) -> T)) -> agent(T).

cast(Agent, Writer) ->
    Native.

-doc "Stops the agent.\n\nInput: one mutable `Agent[T]` receiver.\nOutput: `Unit`.\nTransformation: asks the BEAM runtime to terminate the supervised state\nprocess and release the target-owned handle.".

-spec stop(agent(T)) -> agent(T).

stop(Agent) ->
    Native.

