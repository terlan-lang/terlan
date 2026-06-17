-module(std_core_equal).

-moduledoc "Core equality trait.\n\n`std.core.Equal` defines explicit, type-directed equality for values whose\nequality can be represented safely and portably.".

-export([typer_trait_equal_equal_bool_dict/3, typer_trait_equal_equal_comparison_dict/3, typer_trait_equal_equal_float_dict/3, typer_trait_equal_equal_int_dict/3, typer_trait_equal_equal_string_dict/3, typer_trait_equal_equal_unit_dict/3]).

%% trait Equal.
-doc "Implements equality for `Bool`.\n\nInput: two `Bool` values.\nOutput: `true` when both inputs have the same truth value.\nTransformation: delegates to exact Terlan equality for the closed boolean\ndomain.".

-spec typer_trait_equal_equal_bool_dict(map(), boolean(), boolean()) -> boolean().

typer_trait_equal_equal_bool_dict(_TraitDict, Left, Right) ->
    Left =:= Right.

-doc "Implements equality for `Int`.\n\nInput: two `Int` values.\nOutput: `true` when both inputs represent the same integer.\nTransformation: delegates to exact Terlan equality for the primitive integer\ndomain.".

-spec typer_trait_equal_equal_int_dict(map(), integer(), integer()) -> boolean().

typer_trait_equal_equal_int_dict(_TraitDict, Left, Right) ->
    Left =:= Right.

-doc "Implements equality for `Float`.\n\nInput: two finite `Float` values.\nOutput: `true` when both inputs represent the same finite floating-point\nvalue.\nTransformation: delegates to exact Terlan equality for the finite float\ndomain selected by the source contract.".

-spec typer_trait_equal_equal_float_dict(map(), float(), float()) -> boolean().

typer_trait_equal_equal_float_dict(_TraitDict, Left, Right) ->
    Left =:= Right.

-doc "Implements equality for `String`.\n\nInput: two UTF-8 `String` values.\nOutput: `true` when both inputs contain the same text.\nTransformation: delegates to exact Terlan equality for the primitive string\ndomain.".

-spec typer_trait_equal_equal_string_dict(map(), binary(), binary()) -> boolean().

typer_trait_equal_equal_string_dict(_TraitDict, Left, Right) ->
    Left =:= Right.

-doc "Implements equality for `Unit`.\n\nInput: two `Unit` values.\nOutput: always `true`.\nTransformation: maps the singleton `Unit` domain to equality without\ninspecting either value.".

-spec typer_trait_equal_equal_unit_dict(map(), std_core_unit:unit(), std_core_unit:unit()) -> boolean().

typer_trait_equal_equal_unit_dict(_TraitDict, _left, _right) ->
    true.

-doc "Implements equality for ordering comparison values.\n\nInput: two `Comparison` values.\nOutput: `true` when both inputs are the same comparison value.\nTransformation: delegates to exact Terlan equality for the closed comparison\ndomain.".

-spec typer_trait_equal_equal_comparison_dict(map(), std_core_ordering:comparison(), std_core_ordering:comparison()) -> boolean().

typer_trait_equal_equal_comparison_dict(_TraitDict, Left, Right) ->
    Left =:= Right.

