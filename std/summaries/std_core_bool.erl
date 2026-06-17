-module(std_core_bool).

-moduledoc "Core boolean conformance helpers.\n\n`std.core.Bool` provides named operations used by stdlib trait/conformance\nimplementations for `Bool`.".

-export([compare/2, equal/2, from_string/1, is_false/1, is_true/1, to_string/1]).

-doc "Returns whether two `Bool` values are equal.\n\nInput: two `Bool` values.\nOutput: `true` when both inputs have the same truth value; otherwise\n`false`.\nTransformation: delegates to exact Terlan equality for the two boolean\nvalues.\n\nThis function exists as a named implementation hook for future\n`Comparable[Bool]` / equality-style trait conformance. Ordinary Terlan code\nmay use the `==` operator directly.\n\n```terlan\nmodule bool_equal_example.\n\npub equal(Left: Bool, Right: Bool): Bool ->\n   Left == Right.\n\npub demo(Value: Bool): Bool ->\n   equal(Value, Value).\n```".

-spec equal(boolean(), boolean()) -> boolean().

equal(Left, Right) ->
    Left =:= Right.

-doc "Returns whether a `Bool` value is true.\n\nInput: one `Bool` value.\nOutput: `true` when the input is `true`; otherwise `false`.\nTransformation: returns the input unchanged under a predicate-style helper\nname that can be imported or passed around without using operator syntax.\n\n```terlan\nmodule bool_is_true_example.\n\npub demo(value: Bool): Bool ->\n    std.core.Bool.is_true(value).\n```".

-spec is_true(boolean()) -> boolean().

is_true(Value) ->
    Value.

-doc "Returns whether a `Bool` value is false.\n\nInput: one `Bool` value.\nOutput: `true` when the input is `false`; otherwise `false`.\nTransformation: compares the input against the canonical `false` value.\n\n```terlan\nmodule bool_is_false_example.\n\npub demo(value: Bool): Bool ->\n    std.core.Bool.is_false(value).\n```".

-spec is_false(boolean()) -> boolean().

is_false(Value) ->
    Value =:= false.

-doc "Compares two `Bool` values using the canonical total ordering.\n\nInput: two `Bool` values.\nOutput: `Lt` when `left` is `false` and `right` is `true`, `Eq` when\nboth inputs are equal, and `Gt` when `left` is `true` and `right` is\n`false`.\nTransformation: maps the closed boolean domain onto\n`std.core.Ordering.Comparison` with `false < true`.\n\nThis function is the named implementation hook for compiler-known\n`std.core.Ordering.Ordering[Bool]` conformance in 0.0.1.\n\n```terlan\nmodule bool_compare_example.\n\npub demo(): std.core.Ordering.Comparison ->\n   std.core.Bool.compare(false, true).\n```".

-spec compare(boolean(), boolean()) -> std_core_ordering:comparison().

compare(Left, Right) ->
    case Left =:= Right of
    true -> 'eq';
    false -> case Left of
    false -> 'lt';
    true -> 'gt'
end
end.

-doc "Renders a `Bool` as its canonical UTF-8 `String`.\n\nInput: one `Bool` value.\nOutput: `\"true\"` for `true` and `\"false\"` for `false`.\nTransformation: maps the closed boolean runtime domain onto the canonical\nstring spellings required by `std.core.String.Show[Bool]`.\n\n```terlan\nmodule bool_to_string_example.\n\npub demo(): String ->\n   std.core.Bool.to_string(true).\n```".

-spec to_string(boolean()) -> binary().

to_string(Value) ->
    case Value of
    true -> "true";
    false -> "false"
end.

-doc "Parses a canonical UTF-8 `String` into a `Bool`.\n\nInput: one `String` value.\nOutput: `Option[Bool]`, where `\"true\"` returns `Some(true)`, `\"false\"`\nreturns `Some(false)`, and any other string returns `None`.\nTransformation: recognizes only the canonical spellings used by\n`std.core.String.Parse[Bool]` and rejects all other strings.\n\n```terlan\nmodule bool_from_string_example.\n\npub demo(): Option[Bool] ->\n   std.core.Bool.from_string(\"true\").\n```".

-spec from_string(binary()) -> std_core_option:typer_option(boolean()).

from_string(Value) ->
    case Value =:= "true" of
    true -> {'some', true};
    false -> case Value =:= "false" of
    true -> {'some', false};
    false -> 'none'
end
end.

