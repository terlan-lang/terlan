-module(std_beam_task).

-moduledoc "BEAM Task contract.\n\n`std.beam.Task` models a BEAM-targeted supervised process that performs one\nunit of typed work. It is distinct from the portable `std.core.Task`\ncontract: this module is explicitly tied to BEAM process ownership and is\nresolved only by BEAM-capable target profiles.".

-export([cancel/1, result/1, start/1]).

-export_type([task/1]).

-doc "Task represents a supervised BEAM work process.\n\nInput: type parameter `T`.\nOutput: an opaque BEAM-owned process handle that eventually produces `T`.\nTransformation: keeps process identity, mailbox mechanics, and supervision\ntarget-owned while exposing a typed task handle to Terlan source.".

-opaque task(_T) :: term().

-doc "Starts supervised BEAM work immediately.\n\nInput: one zero-argument function producing `T`.\nOutput: `Result[Task[T], Error]`.\nTransformation: asks the BEAM runtime to spawn supervised work and\nnormalizes startup failure into the standard typed error channel.".

-spec start(fun(() -> T)) -> std_core_result:result(task(T), std_core_error:error()).

start(Work) ->
    Native.

-doc "Reads the task result.\n\nInput: one `Task[T]` receiver.\nOutput: `Result[T, Error]`.\nTransformation: waits for or observes the supervised work result through a\ntyped success-or-error value without exposing BEAM monitor messages.".

-spec result(task(T)) -> std_core_result:result(T, std_core_error:error()).

result(Task) ->
    Native.

-doc "Cancels the task.\n\nInput: one mutable `Task[T]` receiver.\nOutput: `Unit`.\nTransformation: asks the BEAM runtime to stop the supervised work process\nwhile keeping shutdown mechanics behind the target-owned runtime surface.".

-spec cancel(task(T)) -> task(T).

cancel(Task) ->
    Native.

