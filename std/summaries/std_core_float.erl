-module(std_core_float).

-moduledoc "Core float conformance helpers.\n\n`std.core.Float` provides named operations used by stdlib trait/conformance\nimplementations for primitive `Float`.".

-export([abs/1, compare/2, equal/2, from_string/1, max/2, min/2, to_string/1]).

-doc "Returns whether two finite `Float` values are equal.\n\nInput: two supported finite `Float` values.\nOutput: `true` when both inputs represent the same finite floating-point\nvalue; otherwise `false`.\nTransformation: delegates to exact Terlan equality for the finite float\ndomain selected by the source contract without changing either input.\n\n```terlan\nmodule float_equal_example.\n\npub demo(): Bool ->\n    std.core.Float.equal(1.5, 1.5).\n```".

-spec equal(float(), float()) -> boolean().

equal(Left, Right) ->
    Left =:= Right.

-doc "Returns the smaller of two finite `Float` values.\n\nInput: two supported finite `Float` values.\nOutput: `left` when it is less than or equal to `right`; otherwise `right`.\nTransformation: compares the two inputs with primitive finite-float\nordering and returns one original input unchanged.\n\n```terlan\nmodule float_min_example.\n\npub demo(): Float ->\n    std.core.Float.min(1.5, 2.25).\n```".

-spec min(float(), float()) -> float().

min(Left, Right) ->
    case Left =< Right of
    true -> Left;
    false -> Right
end.

-doc "Returns the larger of two finite `Float` values.\n\nInput: two supported finite `Float` values.\nOutput: `left` when it is greater than or equal to `right`; otherwise\n`right`.\nTransformation: compares the two inputs with primitive finite-float\nordering and returns one original input unchanged.\n\n```terlan\nmodule float_max_example.\n\npub demo(): Float ->\n    std.core.Float.max(2.25, 1.5).\n```".

-spec max(float(), float()) -> float().

max(Left, Right) ->
    case Left >= Right of
    true -> Left;
    false -> Right
end.

-doc "Returns the absolute value of a finite `Float`.\n\nInput: one supported finite `Float` value.\nOutput: the non-negative magnitude of the input.\nTransformation: returns the input unchanged when it is non-negative;\notherwise subtracts it from zero.\n\n```terlan\nmodule float_abs_example.\n\npub demo(): Float ->\n    std.core.Float.abs(-2.25).\n```".

-spec abs(float()) -> float().

abs(Value) ->
    case Value < 0 of
    true -> 0 - Value;
    false -> Value
end.

-doc "Compares two supported finite `Float` values using numeric ordering.\n\nInput: two supported finite `Float` values.\nOutput: `Lt` when `left` is numerically less than `right`, `Eq` when the\nvalues are equal, and `Gt` when `left` is numerically greater than\n`right`.\nTransformation: maps the selected finite float ordering relation onto\n`std.core.Ordering.Comparison`.\n\nThis function is the named implementation hook for compiler-known\n`std.core.Ordering.Ordering[Float]` conformance in 0.0.1. Non-finite values\nare outside the 0.0.1 source contract.\n\n```terlan\nmodule float_compare_example.\n\npub demo(): std.core.Ordering.Comparison ->\n   std.core.Float.compare(1.5, 2.5).\n```".

-spec compare(float(), float()) -> std_core_ordering:comparison().

compare(Left, Right) ->
    case Left =:= Right of
    true -> 'eq';
    false -> case Left < Right of
    true -> 'lt';
    false -> 'gt'
end
end.

-doc "Renders a finite `Float` as its canonical UTF-8 `String`.\n\nInput: one supported finite `Float` value.\nOutput: the compact decimal string for the input float.\nTransformation: lowers through the compiler-owned `core.float.to_string`\nintrinsic with compact decimal formatting for `std.core.String.Show[Float]`.\n\n```terlan\nmodule float_to_string_example.\n\npub demo(): String ->\n   std.core.Float.to_string(1.5).\n```".

-spec to_string(float()) -> binary().

to_string(Value) ->
    erlang:float_to_list(Value, [{'decimals', 16}, 'compact']).

-doc "Parses a canonical UTF-8 `String` into `Float`.\n\nInput: one `String` value.\nOutput: `Option[Float]`, where supported finite decimal float text returns\n`Some(value)` and invalid text returns `None`.\nTransformation: lowers through the compiler-owned `core.float.from_string`\nintrinsic and accepts only inputs where the full string is consumed.\n\n```terlan\nmodule float_from_string_example.\n\npub demo(): Option[Float] ->\n   std.core.Float.from_string(\"1.5\").\n```".

-spec from_string(binary()) -> std_core_option:typer_option(float()).

from_string(Value) ->
    case string:to_float(Value) of
    {Parsed, Rest} when Rest =:= "" -> {'some', Parsed};
    _ -> 'none'
end.

