-module(std_core_task).

-moduledoc "Core typed asynchronous work contract.\n\n`std.core.Task` represents work that will eventually produce a value or a\ntyped recoverable error. It is the portable source-level async surface for\nTerlan; target profiles decide whether the contract lowers to BEAM workers,\nRust/Tokio futures, JavaScript promises, native worker commands, or an\nunsupported-target diagnostic.".

-export([deferred/1, done/1, failed/1, map/2, recover/2, result/1, spawn/1, then/2]).

-export_type([task/1]).

-doc "Task represents asynchronous work that produces `T`.\n\nInput: type parameter `T`.\nOutput: an opaque task handle whose runtime representation is target-owned.\nTransformation: hides BEAM, Rust, JavaScript, or native worker mechanics\nbehind a typed value that can be composed without source-level `async`,\n`await`, `go`, send, or receive syntax.".

-opaque task(_T) :: term().

-doc "Creates an already-successful task.\n\nInput: one value of type `T`.\nOutput: a `Task[T]` that is complete with that value.\nTransformation: wraps an immediate value in the selected target's completed\ntask representation without starting runtime work.".

-spec done(T) -> task(T).

done(Value) ->
    {'task_done', Value}.

-doc "Creates an already-failed task.\n\nInput: one portable `Error`.\nOutput: a failed `Task[T]`.\nTransformation: wraps a typed recoverable error in the selected target's\nfailed task representation without throwing a target exception.".

-spec failed(std_core_error:error()) -> task(_T).

failed(Error) ->
    Native.

-doc "Creates a lazy task from work that has not started.\n\nInput: one zero-argument function producing `T`.\nOutput: a `Task[T]` that records the work without starting it immediately.\nTransformation: preserves the distinction between a lazy task and an\nalready-running task so target profiles can enforce their scheduling model.".

-spec deferred(fun(() -> T)) -> task(T).

deferred(Work) ->
    Native.

-doc "Starts runtime work immediately.\n\nInput: one zero-argument function producing `T`.\nOutput: a `Task[T]` representing the started work.\nTransformation: delegates scheduling to the selected target profile. This is\nintentionally a named effectful function, not constructor syntax, so source\ncode makes the start boundary explicit.".

-spec spawn(fun(() -> T)) -> task(T).

spawn(Work) ->
    Native.

-doc "Maps a successful task value.\n\nInput: one task and one mapper from `T` to `U`.\nOutput: a `Task[U]`.\nTransformation: composes the mapper with the task's success channel while\npreserving the original error channel.".

-spec map(task(T), fun((T) -> U)) -> task(U).

map(Task, Mapper) ->
    Native.

-doc "Chains a task into another task-producing computation.\n\nInput: one task and one continuation from `T` to `Task[U]`.\nOutput: a flattened `Task[U]`.\nTransformation: provides monadic bind for task sequencing without forcing\nnested callback chains into the user-facing source.".

-spec then(task(T), fun((T) -> task(U))) -> task(U).

then(Task, Next) ->
    Native.

-doc "Recovers a failed task into a success value.\n\nInput: one task and one handler from `Error` to `T`.\nOutput: a `Task[T]` that succeeds with the handler value when the original\ntask fails.\nTransformation: maps the error channel into the success channel without\nexposing target-specific exceptions.".

-spec recover(task(T), fun((error()) -> T)) -> task(T).

recover(Task, Handler) ->
    Native.

-doc "Joins a task into a typed result.\n\nInput: one task.\nOutput: `Result[T, Error]`.\nTransformation: waits or observes according to the active target profile and\nreturns a typed success-or-error value. Unsupported targets reject this\noperation before backend emission.".

-spec result(task(T)) -> std_core_result:result(T, std_core_error:error()).

result(Task) ->
    case Task of
    {'task_done', Value} -> {ok, Value}
end.

