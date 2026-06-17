-module(std_core_int).

-moduledoc "Core integer conformance helpers.\n\n`std.core.Int` provides named operations used by stdlib trait/conformance\nimplementations for primitive `Int`.".

-export([abs/1, compare/2, equal/2, from_string/1, max/2, min/2, to_string/1]).

-doc "Returns whether two `Int` values are equal.\n\nInput: two `Int` values.\nOutput: `true` when both inputs represent the same integer; otherwise\n`false`.\nTransformation: delegates to exact Terlan equality for the primitive\ninteger domain without changing either input.\n\n```terlan\nmodule int_equal_example.\n\npub demo(): Bool ->\n    std.core.Int.equal(42, 42).\n```".

-spec equal(integer(), integer()) -> boolean().

equal(Left, Right) ->
    Left =:= Right.

-doc "Returns the smaller of two `Int` values.\n\nInput: two `Int` values.\nOutput: `left` when it is less than or equal to `right`; otherwise `right`.\nTransformation: compares the two inputs with primitive integer ordering and\nreturns one original input unchanged.\n\n```terlan\nmodule int_min_example.\n\npub demo(): Int ->\n    std.core.Int.min(3, 7).\n```".

-spec min(integer(), integer()) -> integer().

min(Left, Right) ->
    case Left =< Right of
    true -> Left;
    false -> Right
end.

-doc "Returns the larger of two `Int` values.\n\nInput: two `Int` values.\nOutput: `left` when it is greater than or equal to `right`; otherwise\n`right`.\nTransformation: compares the two inputs with primitive integer ordering and\nreturns one original input unchanged.\n\n```terlan\nmodule int_max_example.\n\npub demo(): Int ->\n    std.core.Int.max(7, 3).\n```".

-spec max(integer(), integer()) -> integer().

max(Left, Right) ->
    case Left >= Right of
    true -> Left;
    false -> Right
end.

-doc "Returns the absolute value of an `Int`.\n\nInput: one `Int` value.\nOutput: the non-negative magnitude of the input.\nTransformation: returns the input unchanged when it is non-negative;\notherwise subtracts it from zero.\n\n```terlan\nmodule int_abs_example.\n\npub demo(): Int ->\n    std.core.Int.abs(-7).\n```".

-spec abs(integer()) -> integer().

abs(Value) ->
    case Value < 0 of
    true -> 0 - Value;
    false -> Value
end.

-doc "Compares two `Int` values using numeric total ordering.\n\nInput: two `Int` values.\nOutput: `Lt` when `left` is numerically less than `right`, `Eq` when the\nvalues are equal, and `Gt` when `left` is numerically greater than\n`right`.\nTransformation: maps the primitive integer ordering relation onto\n`std.core.Ordering.Comparison`.\n\nThis function is the named implementation hook for compiler-known\n`std.core.Ordering.Ordering[Int]` conformance in 0.0.1.\n\n```terlan\nmodule int_compare_example.\n\npub demo(): std.core.Ordering.Comparison ->\n   std.core.Int.compare(1, 2).\n```".

-spec compare(integer(), integer()) -> std_core_ordering:comparison().

compare(Left, Right) ->
    case Left =:= Right of
    true -> 'eq';
    false -> case Left < Right of
    true -> 'lt';
    false -> 'gt'
end
end.

-doc "Renders an `Int` as its canonical UTF-8 `String`.\n\nInput: one `Int` value.\nOutput: the canonical decimal string for the input integer.\nTransformation: lowers through the compiler-owned `core.int.to_string`\nintrinsic and returns the canonical text representation used by\n`std.core.String.Show[Int]`.\n\n```terlan\nmodule int_to_string_example.\n\npub demo(): String ->\n   std.core.Int.to_string(42).\n```".

-spec to_string(integer()) -> binary().

to_string(Value) ->
    erlang:integer_to_list(Value).

-doc "Parses a canonical UTF-8 `String` into `Int`.\n\nInput: one `String` value.\nOutput: `Option[Int]`, where canonical signed decimal text returns\n`Some(value)` and invalid text returns `None`.\nTransformation: lowers through the compiler-owned `core.int.from_string`\nintrinsic and accepts only inputs where the full string is consumed.\n\n```terlan\nmodule int_from_string_example.\n\npub demo(): Option[Int] ->\n   std.core.Int.from_string(\"42\").\n```".

-spec from_string(binary()) -> std_core_option:typer_option(integer()).

from_string(Value) ->
    case string:to_integer(Value) of
    {Parsed, Rest} when Rest =:= "" -> {'some', Parsed};
    _ -> 'none'
end.

