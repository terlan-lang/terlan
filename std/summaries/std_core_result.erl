-module(std_core_result).

-moduledoc "Core success-or-error value type.\n\n`std.core.Result` represents either a successful value or an error value.".

-export([and_then/2, is_err/1, is_ok/1, map/2, map_error/2, with_default/2]).

-export_type([err/1, ok/1, result/2]).

-doc "Ok represents a successful result value.\n\n`Ok[T]` is a typed alias for the tuple `{Atom[\"ok\"], value}`.\n\n```terlan\nmodule ok_type_example.\n\npub type Ok[T] =\n   {Atom[\"ok\"], value: T}.\n\npub success(value: Int): Ok[Int] ->\n   Ok(value).\n```".

-type ok(T) :: {ok, T}.

-doc "Err represents a failed result value.\n\n`Err[E]` is a typed alias for the tuple `{Atom[\"error\"], reason}`.\n\n```terlan\nmodule err_type_example.\n\npub type Err[E] =\n   {Atom[\"error\"], reason: E}.\n\npub type Problem = Atom[\"problem\"].\n\npub failure(reason: Problem): Err[Problem] ->\n   Err(reason).\n```".

-type err(E) :: {error, E}.

-doc "Result represents a computation that can succeed or fail.\n\nUse `Ok(value)` for success and `Err(reason)` for failure.\n\n```terlan\nmodule result_example.\n\npub type Ok[T] =\n   {Atom[\"ok\"], value: T}.\n\npub type Err[E] =\n   {Atom[\"error\"], reason: E}.\n\npub type Problem =\n   Atom[\"problem\"].\n\npub type Result[T, E] =\n     Ok[T]\n   | Err[E].\n\npub success(x: Int): Result[Int, Problem] ->\n   Ok(x).\n\npub failure(problem: Problem): Result[Int, Problem] ->\n   Err(problem).\n```".

-type result(T, E) :: ok(T) | err(E).

-doc "Returns whether a result is successful.\n\nInput: one `Result[A, E]`.\nOutput: `true` for `Ok(_)`, otherwise `false`.\nTransformation: pattern matches the result shape without inspecting the\nsuccess value or error value.".

-spec is_ok(std_core_result:result(_A, _E)) -> boolean().

is_ok(Value) ->
    case Value of
    {ok, _x} -> true;
    {error, _reason} -> false
end.

-doc "Returns whether a result is an error.\n\nInput: one `Result[A, E]`.\nOutput: `true` for `Err(_)`, otherwise `false`.\nTransformation: pattern matches the result shape without inspecting the\nsuccess value or error value.".

-spec is_err(std_core_result:result(_A, _E)) -> boolean().

is_err(Value) ->
    case Value of
    {ok, _x} -> false;
    {error, _reason} -> true
end.

-doc "Maps a successful value and leaves errors unchanged.\n\n```terlan\nmodule result_map_example.\n\nimport std.core.Result.{Err, Ok}.\nimport type std.core.Result.Result.\n\npub map(value: Result[A, E], f: (A) -> B): Result[B, E] ->\n   case value {\n       Ok(x) ->\n           Ok(f(x));\n\n       Err(reason) ->\n           Err(reason)\n   }.\n\npub demo(value: Result[Int, E], f: (Int) -> Int): Result[Int, E] ->\n   map(value, f).\n```".

-spec map(std_core_result:result(A, E), fun((A) -> B)) -> std_core_result:result(B, E).

map(Value, F) ->
    case Value of
    {ok, X} -> {ok, F(X)};
    {error, Reason} -> {error, Reason}
end.

-doc "Maps an error value and leaves successes unchanged.\n\n```terlan\nmodule result_map_error_example.\n\nimport std.core.Result.{Err, Ok}.\nimport type std.core.Result.Result.\n\npub map_error(value: Result[A, E], f: (E) -> G): Result[A, G] ->\n   case value {\n       Ok(x) ->\n           Ok(x);\n\n       Err(reason) ->\n           Err(f(reason))\n   }.\n\npub demo(value: Result[Int, Int], f: (Int) -> Int): Result[Int, Int] ->\n   map_error(value, f).\n```".

-spec map_error(std_core_result:result(A, E), fun((E) -> G)) -> std_core_result:result(A, G).

map_error(Value, F) ->
    case Value of
    {ok, X} -> {ok, X};
    {error, Reason} -> {error, F(Reason)}
end.

-doc "Chains a successful value into another result-producing computation.\n\n`and_then` returns the original error unchanged when the input is an error.\nWhen the input succeeds, it calls the supplied function and returns that\nfunction's result directly.\n\n```terlan\nmodule result_and_then_example.\n\nimport std.core.Result.{Err, Ok}.\nimport type std.core.Result.Result.\n\npub and_then(value: Result[A, E], f: (A) -> Result[B, E]): Result[B, E] ->\n   case value {\n       Ok(x) ->\n           f(x);\n\n       Err(reason) ->\n           Err(reason)\n   }.\n\npub demo(value: Result[Int, E], f: (Int) -> Result[Int, E]): Result[Int, E] ->\n   and_then(value, f).\n```".

-spec and_then(std_core_result:result(A, E), fun((A) -> std_core_result:result(B, E))) -> std_core_result:result(B, E).

and_then(Value, F) ->
    case Value of
    {ok, X} -> F(X);
    {error, Reason} -> {error, Reason}
end.

-doc "Extracts a successful value or returns the provided default.\n\n```terlan\nmodule result_with_default_example.\n\nimport std.core.Result.{Err, Ok}.\nimport type std.core.Result.Result.\n\npub with_default(value: Result[A, E], default: A): A ->\n   case value {\n       Ok(x) ->\n           x;\n\n       Err(_reason) ->\n           default\n   }.\n\npub demo(value: Result[Int, E]): Int ->\n   with_default(value, 0).\n```".

-spec with_default(std_core_result:result(A, _E), A) -> A.

with_default(Value, Default) ->
    case Value of
    {ok, X} -> X;
    {error, _reason} -> Default
end.

