-module(std_core_string).

-moduledoc "Core UTF-8 string contract.\n\n`std.core.String` defines Terlan's target-neutral text surface. `String` is\na primitive source-level UTF-8 value type; each backend owns the runtime\nrepresentation, but every backend must preserve this module's public\nbehavior.\n\nThis module exposes conversion traits, receiver methods on the intrinsic\n`String` type, and module functions for operations that naturally consume\ncollections. Public operations lower through compiler-owned intrinsics so\nportable Terlan source never depends on Erlang modules, JavaScript prototype\nmethods, Rust crate APIs, or platform-specific encodings.\n\nTarget-specific string types, such as a future `std.js.JsString`, must not\nextend or redefine core `String`. They should expose their own platform API\nand use explicit conversion traits when crossing into or out of core\n`String`.\n\nDocumentation contract:\n- `//!` comments document the module and are intended to lower to BEAM\n `-moduledoc` when targeting HexDocs-native output.\n- `///` comments document public types, traits, functions, and receiver\n methods and are intended to lower to BEAM `-doc`.\n- Every public API documents its inputs, outputs, and transformation.\n- Examples use canonical Terlan source syntax and should become executable\n std tests as the documentation pipeline hardens.".

-export([append/2, byte_size/1, compare/2, concat/1, contains/2, ends_with/2, equal/2, from_string/1, is_empty/1, length/1, lowercase/1, replace/3, split/2, split_once/2, starts_with/2, to_string/1, trim/1, trim_end/1, trim_start/1, typer_trait_parse_from_string_bool_dict/2, typer_trait_parse_from_string_float_dict/2, typer_trait_parse_from_string_int_dict/2, typer_trait_parse_from_string_string_dict/2, typer_trait_parse_from_string_unit_dict/2, typer_trait_show_to_string_bool_dict/2, typer_trait_show_to_string_float_dict/2, typer_trait_show_to_string_int_dict/2, typer_trait_show_to_string_string_dict/2, typer_trait_show_to_string_unit_dict/2, uppercase/1]).

%% trait Show.
%% trait Parse.
-doc "Implements `Show` for `Bool`.\n\nInput: one `Bool` value.\nOutput: the canonical UTF-8 `String` representation of the boolean.\nTransformation: delegates to the primitive `std.core.Bool.to_string`\nconversion hook.".

-spec typer_trait_show_to_string_bool_dict(map(), boolean()) -> binary().

typer_trait_show_to_string_bool_dict(_TraitDict, Value) ->
    case Value of
    true -> "true";
    false -> "false"
end.

-doc "Implements `Show` for `Int`.\n\nInput: one `Int` value.\nOutput: the canonical decimal UTF-8 `String` representation of the integer.\nTransformation: delegates to the primitive `std.core.Int.to_string`\nconversion hook.".

-spec typer_trait_show_to_string_int_dict(map(), integer()) -> binary().

typer_trait_show_to_string_int_dict(_TraitDict, Value) ->
    erlang:integer_to_list(Value).

-doc "Implements `Show` for `Float`.\n\nInput: one `Float` value.\nOutput: the canonical finite UTF-8 `String` representation of the float.\nTransformation: delegates to the primitive `std.core.Float.to_string`\nconversion hook.".

-spec typer_trait_show_to_string_float_dict(map(), float()) -> binary().

typer_trait_show_to_string_float_dict(_TraitDict, Value) ->
    erlang:float_to_list(Value, [{'decimals', 16}, 'compact']).

-doc "Implements `Show` for `String`.\n\nInput: one `String` value.\nOutput: the unchanged input string.\nTransformation: preserves the value because `String` is already textual.".

-spec typer_trait_show_to_string_string_dict(map(), binary()) -> binary().

typer_trait_show_to_string_string_dict(_TraitDict, Value) ->
    Value.

-doc "Implements `Show` for `Unit`.\n\nInput: one `Unit` value.\nOutput: the canonical UTF-8 `String` representation of Unit.\nTransformation: delegates to the primitive `std.core.Unit.to_string`\nconversion hook.".

-spec typer_trait_show_to_string_unit_dict(map(), unit) -> binary().

typer_trait_show_to_string_unit_dict(_TraitDict, Value) ->
    std_core_unit:to_string(Value).

-doc "Implements `Parse` for `Bool`.\n\nInput: one UTF-8 `String` value.\nOutput: `Option[Bool]`, where canonical boolean text returns `Some(value)`\nand invalid text returns `None`.\nTransformation: delegates to the primitive `std.core.Bool.from_string`\nconversion hook.".

-spec typer_trait_parse_from_string_bool_dict(map(), binary()) -> std_core_option:typer_option(boolean()).

typer_trait_parse_from_string_bool_dict(_TraitDict, Value) ->
    case Value of
    Value when Value =:= "true" -> {'some', true};
    Value when Value =:= "false" -> {'some', false};
    _ -> 'none'
end.

-doc "Implements `Parse` for `Int`.\n\nInput: one UTF-8 `String` value.\nOutput: `Option[Int]`, where canonical decimal integer text returns\n`Some(value)` and invalid text returns `None`.\nTransformation: delegates to the primitive `std.core.Int.from_string`\nconversion hook.".

-spec typer_trait_parse_from_string_int_dict(map(), binary()) -> std_core_option:typer_option(integer()).

typer_trait_parse_from_string_int_dict(_TraitDict, Value) ->
    case string:to_integer(Value) of
    {Parsed, Rest} when Rest =:= "" -> {'some', Parsed};
    _ -> 'none'
end.

-doc "Implements `Parse` for `Float`.\n\nInput: one UTF-8 `String` value.\nOutput: `Option[Float]`, where canonical finite float text returns\n`Some(value)` and invalid text returns `None`.\nTransformation: delegates to the primitive `std.core.Float.from_string`\nconversion hook.".

-spec typer_trait_parse_from_string_float_dict(map(), binary()) -> std_core_option:typer_option(float()).

typer_trait_parse_from_string_float_dict(_TraitDict, Value) ->
    case string:to_float(Value) of
    {Parsed, Rest} when Rest =:= "" -> {'some', Parsed};
    _ -> 'none'
end.

-doc "Implements `Parse` for `String`.\n\nInput: one UTF-8 `String` value.\nOutput: `Some(value)` because every `String` is already valid `String`\ncontent.\nTransformation: delegates to the primitive `String.from_string` receiver\nhook, preserving the input and wrapping it in `Option[String]`.".

-spec typer_trait_parse_from_string_string_dict(map(), binary()) -> std_core_option:typer_option(binary()).

typer_trait_parse_from_string_string_dict(_TraitDict, Value) ->
    {'some', Value}.

-doc "Implements `Parse` for `Unit`.\n\nInput: one UTF-8 `String` value.\nOutput: `Option[Unit]`, where canonical Unit text returns `Some(Unit)` and\ninvalid text returns `None`.\nTransformation: delegates to the primitive `std.core.Unit.from_string`\nconversion hook.".

-spec typer_trait_parse_from_string_unit_dict(map(), binary()) -> std_core_option:typer_option(unit).

typer_trait_parse_from_string_unit_dict(_TraitDict, Value) ->
    std_core_unit:from_string(Value).

-doc "Returns whether two `String` values are equal.\n\nInput: two UTF-8 `String` values.\nOutput: `true` when both inputs contain the same text; otherwise `false`.\nTransformation: delegates to exact Terlan equality for the two string\nvalues without changing either input.\n\nThis function exists as the named implementation hook for future\n`Comparable[String]` / equality-style trait conformance. Ordinary Terlan\ncode may use the `==` operator directly.\n\n```terlan\nmodule string_equal_example.\n\npub demo(): Bool ->\n   \"hello\".equal(\"hello\").\n```".

-spec equal(binary(), binary()) -> boolean().

equal(Left, Right) ->
    Left =:= Right.

-doc "Compares two `String` values using the selected stable source ordering.\n\nInput: two UTF-8 `String` values.\nOutput: `Lt` when `left` sorts before `right`, `Eq` when both strings\nare equal, and `Gt` when `left` sorts after `right`.\nTransformation: maps Terlan's selected string ordering relation onto\n`std.core.Ordering.Comparison` without changing either input.\n\nThis function is the named implementation hook for compiler-known\n`std.core.Ordering.Ordering[String]` conformance in 0.0.1. Rich Unicode\ncollation is not part of this hook; targets must preserve the stable source\nordering selected by the compiler contract.\n\n```terlan\nmodule string_compare_example.\n\npub demo(): std.core.Ordering.Comparison ->\n   \"a\".compare(\"b\").\n```".

-spec compare(binary(), binary()) -> std_core_ordering:comparison().

compare(Left, Right) ->
    if
    Left =:= Right -> 'eq';
    Left < Right -> 'lt';
    true -> 'gt'
end.

-doc "Renders a `String` as itself.\n\nInput: one UTF-8 `String` value.\nOutput: the same `String` value.\nTransformation: preserves the input unchanged because `String` is already\nthe canonical textual representation of itself.\n\n```terlan\nmodule string_to_string_example.\n\npub demo(): String ->\n   \"hello\".to_string().\n```".

-spec to_string(binary()) -> binary().

to_string(Value) ->
    Value.

-doc "Parses a `String` into a `String`.\n\nInput: one UTF-8 `String` value.\nOutput: `Some(value)` because every `String` value is already a valid\n`String` representation.\nTransformation: wraps the unchanged input in `Option[String]`.\n\n```terlan\nmodule string_from_string_example.\n\npub demo(): Option[String] ->\n   \"hello\".from_string().\n```".

-spec from_string(binary()) -> std_core_option:typer_option(binary()).

from_string(Value) ->
    {'some', Value}.

-doc "Returns whether a `String` contains no text.\n\nInput: one UTF-8 `String` value.\nOutput: `true` when the string is empty; otherwise `false`.\nTransformation: compares the input with the canonical empty string literal.\n\n```terlan\nmodule string_is_empty_example.\n\npub demo(): Bool ->\n   \"\".is_empty().\n```".

-spec is_empty(binary()) -> boolean().

is_empty(Value) ->
    Value =:= "".

-doc "Appends two `String` values.\n\nInput: two UTF-8 `String` values.\nOutput: one `String` containing `left` followed by `right`.\nTransformation: lowers through the compiler-owned `core.string.append`\nintrinsic for each target backend.\n\n```terlan\nmodule string_append_example.\n\npub demo(): String ->\n   \"hello\".append(\"world\").\n```".

-spec append(binary(), binary()) -> binary().

append(Left, Right) ->
    string:concat(Left, Right).

-doc "Concatenates a list of `String` values.\n\nInput: a `List[String]`.\nOutput: one `String` containing each input string in list order.\nTransformation: lowers through the compiler-owned `core.string.concat`\nintrinsic for each target backend.\n\n```terlan\nmodule string_concat_example.\n\npub demo(): String ->\n   std.core.String.concat([\"hello\", \"world\"]).\n```".

-spec concat([binary()]) -> binary().

concat(Values) ->
    lists:append(Values).

-doc "Returns whether a `String` contains a pattern.\n\nInput: a source `String` and a pattern `String`.\nOutput: `true` when the pattern is present; otherwise `false`.\nTransformation: lowers through the compiler-owned `core.string.contains`\nintrinsic for each target backend.\n\n```terlan\nmodule string_contains_example.\n\npub demo(): Bool ->\n   \"hello\".contains(\"ell\").\n```".

-spec contains(binary(), binary()) -> boolean().

contains(Value, Pattern) ->
    case string:find(Value, Pattern) of
    'nomatch' -> false;
    _ -> true
end.

-doc "Returns whether a `String` starts with a prefix.\n\nInput: a source `String` and a prefix `String`.\nOutput: `true` when the prefix is present at the start; otherwise `false`.\nTransformation: lowers through the compiler-owned `core.string.starts_with`\nintrinsic for each target backend.\n\n```terlan\nmodule string_starts_with_example.\n\npub demo(): Bool ->\n   \"hello\".starts_with(\"he\").\n```".

-spec starts_with(binary(), binary()) -> boolean().

starts_with(Value, Prefix) ->
    case string:prefix(Value, Prefix) of
    'nomatch' -> false;
    _ -> true
end.

-doc "Returns whether a `String` ends with a suffix.\n\nInput: a source `String` and a suffix `String`.\nOutput: `true` when the suffix is present at the end; otherwise `false`.\nTransformation: lowers through the compiler-owned `core.string.ends_with`\nintrinsic for each target backend. The empty suffix is accepted for every\nstring.\n\n```terlan\nmodule string_ends_with_example.\n\npub demo(): Bool ->\n   \"hello\".ends_with(\"lo\").\n```".

-spec ends_with(binary(), binary()) -> boolean().

ends_with(Value, Suffix) ->
    if
    Suffix =:= "" -> true;
    true -> case string:find(Value, Suffix, 'trailing') of
    'nomatch' -> false;
    Found -> Found =:= Suffix
end
end.

-doc "Counts user-visible text units in a `String`.\n\nInput: one UTF-8 `String` value.\nOutput: the selected portable text length for the text.\nTransformation: lowers through the compiler-owned `core.string.length`\nintrinsic for each target backend.\n\n```terlan\nmodule string_length_example.\n\npub demo(): Int ->\n   \"hello\".length().\n```".

-spec length(binary()) -> integer().

length(Value) ->
    string:length(Value).

-doc "Counts encoded bytes in a `String`.\n\nInput: one UTF-8 `String` value.\nOutput: the number of bytes in the canonical UTF-8 representation.\nTransformation: lowers through the compiler-owned `core.string.byte_size`\nintrinsic for each target backend.\n\n```terlan\nmodule string_byte_size_example.\n\npub demo(): Int ->\n   \"hello\".byte_size().\n```".

-spec byte_size(binary()) -> integer().

byte_size(Value) ->
    erlang:byte_size(unicode:characters_to_binary(Value)).

-doc "Converts a `String` to lowercase.\n\nInput: one UTF-8 `String` value.\nOutput: the lowercase form of the string.\nTransformation: lowers through the compiler-owned `core.string.lowercase`\nintrinsic for each target backend.\n\n```terlan\nmodule string_lowercase_example.\n\npub demo(): String ->\n   \"HELLO\".lowercase().\n```".

-spec lowercase(binary()) -> binary().

lowercase(Value) ->
    string:lowercase(Value).

-doc "Converts a `String` to uppercase.\n\nInput: one UTF-8 `String` value.\nOutput: the uppercase form of the string.\nTransformation: lowers through the compiler-owned `core.string.uppercase`\nintrinsic for each target backend.\n\n```terlan\nmodule string_uppercase_example.\n\npub demo(): String ->\n   \"hello\".uppercase().\n```".

-spec uppercase(binary()) -> binary().

uppercase(Value) ->
    string:uppercase(Value).

-doc "Removes leading and trailing whitespace from a `String`.\n\nInput: one UTF-8 `String` value.\nOutput: the string with surrounding whitespace removed.\nTransformation: lowers through the compiler-owned `core.string.trim`\nintrinsic for each target backend.\n\n```terlan\nmodule string_trim_example.\n\npub demo(): String ->\n   \" hello \".trim().\n```".

-spec trim(binary()) -> binary().

trim(Value) ->
    string:trim(Value).

-doc "Removes leading whitespace from a `String`.\n\nInput: one UTF-8 `String` value.\nOutput: the string with leading whitespace removed.\nTransformation: lowers through the compiler-owned `core.string.trim_start`\nintrinsic for each target backend.\n\n```terlan\nmodule string_trim_start_example.\n\npub demo(): String ->\n   \" hello \".trim_start().\n```".

-spec trim_start(binary()) -> binary().

trim_start(Value) ->
    string:trim(Value, 'leading').

-doc "Removes trailing whitespace from a `String`.\n\nInput: one UTF-8 `String` value.\nOutput: the string with trailing whitespace removed.\nTransformation: lowers through the compiler-owned `core.string.trim_end`\nintrinsic for each target backend.\n\n```terlan\nmodule string_trim_end_example.\n\npub demo(): String ->\n   \" hello \".trim_end().\n```".

-spec trim_end(binary()) -> binary().

trim_end(Value) ->
    string:trim(Value, 'trailing').

-doc "Replaces every occurrence of a pattern in a `String`.\n\nInput: a source `String`, a pattern `String`, and a replacement `String`.\nOutput: a `String` where every occurrence of `pattern` has been replaced\nby `replacement`.\nTransformation: lowers through the compiler-owned `core.string.replace`\nintrinsic for each target backend.\n\n```terlan\nmodule string_replace_example.\n\npub demo(): String ->\n   \"hello\".replace(\"l\", \"x\").\n```".

-spec replace(binary(), binary(), binary()) -> binary().

replace(Value, Pattern, Replacement) ->
    lists:flatten(string:replace(Value, Pattern, Replacement, 'all')).

-doc "Splits a `String` on every occurrence of a separator.\n\nInput: a source `String` and separator `String`.\nOutput: a `List[String]` of split segments.\nTransformation: lowers through the compiler-owned `core.string.split`\nintrinsic for each target backend.\n\n```terlan\nmodule string_split_example.\n\npub demo(): List[String] ->\n   \"a,b\".split(\",\").\n```".

-spec split(binary(), binary()) -> [binary()].

split(Value, On) ->
    string:split(Value, On, 'all').

-doc "Splits a `String` on the first occurrence of a separator.\n\nInput: a source `String` and separator `String`.\nOutput: `Some({left, right})` when the separator is present; otherwise\n`None`.\nTransformation: lowers through the compiler-owned `core.string.split_once`\nintrinsic for each target backend.\n\n```terlan\nmodule string_split_once_example.\n\npub demo(): Option[{String, String}] ->\n   \"a,b\".split_once(\",\").\n```".

-spec split_once(binary(), binary()) -> std_core_option:typer_option({binary(), binary()}).

split_once(Value, On) ->
    case string:split(Value, On, 'leading') of
    [Left, Right] -> {'some', {Left, Right}};
    [_] -> 'none'
end.

