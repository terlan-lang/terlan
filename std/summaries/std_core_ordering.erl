-module(std_core_ordering).

-moduledoc "Core ordering trait and comparison result.\n\n`std.core.Ordering` defines the trait for values that can be ordered. The\nclosed comparison result is named `Comparison`.".

-export([compare/2, from_string/1, to_string/1, typer_trait_ordering_compare_bool_dict/3, typer_trait_ordering_compare_comparison_dict/3, typer_trait_ordering_compare_float_dict/3, typer_trait_ordering_compare_int_dict/3, typer_trait_ordering_compare_string_dict/3, typer_trait_ordering_compare_unit_dict/3]).

-export_type([comparison/0, eq/0, gt/0, lt/0]).

-doc "Comparison represents less-than, equal, or greater-than comparison results.\n\nInput: no runtime input; this is a closed result type.\nOutput: one of `Lt`, `Eq`, or `Gt`.\nTransformation: gives ordering implementations a stable return domain\nwithout making the `Ordering` trait itself a value.\n\n```terlan\nmodule comparison_example.\n\npub type Comparison =\n     Lt\n   | Eq\n   | Gt.\n\npub less(): Comparison ->\n   Lt.\n```".

-type lt() :: 'lt'.

-doc "Eq represents equal comparison.\n\n```terlan\nmodule comparison_eq_example.\n\npub equal(): Eq ->\n   Eq.\n```".

-type eq() :: 'eq'.

-doc "Gt represents greater-than comparison.\n\n```terlan\nmodule comparison_gt_example.\n\npub greater(): Gt ->\n   Gt.\n```".

-type gt() :: 'gt'.

-doc "Comparison is the closed result domain for total ordering.\n\n```terlan\nmodule comparison_type_example.\n\npub choose(flag: Bool): Comparison ->\n   if {\n       flag -> Gt;\n       _ -> Lt\n   }.\n```".

-type comparison() :: lt() | eq() | gt().

-doc "Compares two `Comparison` values using declaration order.\n\nInput: two `Comparison` values.\nOutput: `Lt` when `left` comes before `right`, `Eq` when both values are\nequal, and `Gt` when `left` comes after `right`.\nTransformation: orders the closed comparison result domain as\n`Lt < Eq < Gt`.\n\nThis function is the named implementation hook for compiler-known\n`std.core.Ordering.Ordering[Comparison]` conformance in 0.0.1.\n\n```terlan\nmodule comparison_compare_example.\n\npub demo(): Comparison ->\n   std.core.Ordering.compare(Lt, Gt).\n```".

-spec compare(comparison(), comparison()) -> comparison().

compare(Left, Right) ->
    case Left =:= Right of
    true -> 'eq';
    false -> case Left of
    'lt' -> 'lt';
    'eq' -> case Right of
    'lt' -> 'gt';
    'eq' -> 'eq';
    'gt' -> 'lt'
end;
    'gt' -> 'gt'
end
end.

-doc "Renders `Comparison` as its canonical UTF-8 `String`.\n\nInput: one `Comparison` value.\nOutput: `\"lt\"`, `\"eq\"`, or `\"gt\"`.\nTransformation: maps the closed comparison result domain onto the canonical\nstring spellings required by `std.core.String.Show[Comparison]`.\n\n```terlan\nmodule comparison_to_string_example.\n\npub demo(): String ->\n   std.core.Ordering.to_string(Lt).\n```".

-spec to_string(comparison()) -> binary().

to_string(Value) ->
    case Value of
    'lt' -> "lt";
    'eq' -> "eq";
    'gt' -> "gt"
end.

-doc "Parses a canonical UTF-8 `String` into `Comparison`.\n\nInput: one `String` value.\nOutput: `Option[Comparison]`, where `\"lt\"`, `\"eq\"`, and `\"gt\"` return their\ncorresponding comparison values and any other string returns `None`.\nTransformation: recognizes only the canonical spellings used by\n`std.core.String.Parse[Comparison]` and rejects all other strings.\n\n```terlan\nmodule comparison_from_string_example.\n\npub demo(): Option[Comparison] ->\n   std.core.Ordering.from_string(\"lt\").\n```".

-spec from_string(binary()) -> std_core_option:typer_option(comparison()).

from_string(Value) ->
    case Value =:= "lt" of
    true -> {'some', 'lt'};
    false -> case Value =:= "eq" of
    true -> {'some', 'eq'};
    false -> case Value =:= "gt" of
    true -> {'some', 'gt'};
    false -> 'none'
end
end
end.

%% trait Ordering.
-doc "Implements total ordering for `Bool`.\n\nInput: two `Bool` values.\nOutput: `Lt`, `Eq`, or `Gt` using `false < true`.\nTransformation: compares the inputs without changing either value.".

-spec typer_trait_ordering_compare_bool_dict(map(), boolean(), boolean()) -> comparison().

typer_trait_ordering_compare_bool_dict(_TraitDict, Left, Right) ->
    case Left =:= Right of
    true -> 'eq';
    false -> case Left of
    false -> 'lt';
    true -> 'gt'
end
end.

-doc "Implements total ordering for `Int`.\n\nInput: two `Int` values.\nOutput: `Lt`, `Eq`, or `Gt` using numeric integer ordering.\nTransformation: compares the inputs without changing either value.".

-spec typer_trait_ordering_compare_int_dict(map(), integer(), integer()) -> comparison().

typer_trait_ordering_compare_int_dict(_TraitDict, Left, Right) ->
    case Left =:= Right of
    true -> 'eq';
    false -> case Left < Right of
    true -> 'lt';
    false -> 'gt'
end
end.

-doc "Implements total ordering for finite `Float` values.\n\nInput: two finite `Float` values.\nOutput: `Lt`, `Eq`, or `Gt` using numeric float ordering.\nTransformation: compares the inputs without changing either value.".

-spec typer_trait_ordering_compare_float_dict(map(), float(), float()) -> comparison().

typer_trait_ordering_compare_float_dict(_TraitDict, Left, Right) ->
    case Left =:= Right of
    true -> 'eq';
    false -> case Left < Right of
    true -> 'lt';
    false -> 'gt'
end
end.

-doc "Implements total ordering for `String`.\n\nInput: two UTF-8 `String` values.\nOutput: `Lt`, `Eq`, or `Gt` using Terlan's stable source string ordering.\nTransformation: compares the inputs without changing either value.".

-spec typer_trait_ordering_compare_string_dict(map(), binary(), binary()) -> comparison().

typer_trait_ordering_compare_string_dict(_TraitDict, Left, Right) ->
    case Left =:= Right of
    true -> 'eq';
    false -> case Left < Right of
    true -> 'lt';
    false -> 'gt'
end
end.

-doc "Implements total ordering for `Unit`.\n\nInput: two `Unit` values.\nOutput: always `Eq`.\nTransformation: exposes the singleton unit comparison through the\n`Ordering[Unit]` trait without inspecting either input.".

-spec typer_trait_ordering_compare_unit_dict(map(), unit, unit) -> comparison().

typer_trait_ordering_compare_unit_dict(_TraitDict, _left, _right) ->
    'eq'.

-doc "Implements total ordering for `Comparison`.\n\nInput: two `Comparison` values.\nOutput: `Lt`, `Eq`, or `Gt` using declaration order `Lt < Eq < Gt`.\nTransformation: compares the closed comparison result domain without\nchanging either input and exposes that order through\n`std.core.Ordering.Ordering[Comparison]`.".

-spec typer_trait_ordering_compare_comparison_dict(map(), comparison(), comparison()) -> comparison().

typer_trait_ordering_compare_comparison_dict(_TraitDict, Left, Right) ->
    case Left =:= Right of
    true -> 'eq';
    false -> case Left of
    'lt' -> 'lt';
    'eq' -> case Right of
    'lt' -> 'gt';
    'eq' -> 'eq';
    'gt' -> 'lt'
end;
    'gt' -> 'gt'
end
end.

