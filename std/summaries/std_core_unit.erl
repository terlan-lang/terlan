-module(std_core_unit).

-moduledoc "Core unit type.\n\n`std.core.Unit` defines the conventional no-information return value.".

-export([compare/2, equal/2, from_string/1, to_string/1]).

-export_type([unit/0]).

-doc "Unit is the type of the singleton `Atom[\"unit\"]` value.\n\nUse `Unit` for APIs that need to signal completion without returning\nadditional data.\n\n```terlan\nmodule unit_example.\n\npub type Unit = Atom[\"unit\"].\n\npub done(): Unit ->\n   Unit.\n```".

-type unit() :: unit.

-doc "Returns whether two `Unit` values are equal.\n\nInput: two `Unit` values.\nOutput: always `true`.\nTransformation: maps the singleton `Unit` domain to equality without\ninspecting either value because every valid `Unit` value is the same value.\n\n```terlan\nmodule unit_equal_example.\n\npub demo(): Bool ->\n    std.core.Unit.equal(Unit, Unit).\n```".

-spec equal(unit, unit) -> boolean().

equal(_left, _right) ->
    true.

-doc "Compares two `Unit` values.\n\nInput: two `Unit` values.\nOutput: always `Eq`.\nTransformation: maps the singleton `Unit` domain onto\n`std.core.Ordering.Comparison`; because `Unit` has exactly one value, both\ninputs are always equal.\n\nThis function is the named implementation hook for compiler-known\n`std.core.Ordering.Ordering[Unit]` conformance in 0.0.1.\n\n```terlan\nmodule unit_compare_example.\n\npub demo(): std.core.Ordering.Comparison ->\n   std.core.Unit.compare(Unit, Unit).\n```".

-spec compare(unit, unit) -> std_core_ordering:comparison().

compare(_left, _right) ->
    'eq'.

-doc "Renders `Unit` as its canonical UTF-8 `String`.\n\nInput: one `Unit` value.\nOutput: the string `\"unit\"`.\nTransformation: maps the singleton `Unit` value onto the canonical\nspelling required by `std.core.String.Show[Unit]`.\n\n```terlan\nmodule unit_to_string_example.\n\npub demo(): String ->\n   std.core.Unit.to_string(Unit).\n```".

-spec to_string(unit) -> binary().

to_string(Value) ->
    "unit".

-doc "Parses a canonical UTF-8 `String` into `Unit`.\n\nInput: one `String` value.\nOutput: `Option[Unit]`, where `\"unit\"` returns `Some(Unit)` and any other\nstring returns `None`.\nTransformation: recognizes only the canonical spelling used by\n`std.core.String.Parse[Unit]` and rejects all other strings.\n\n```terlan\nmodule unit_from_string_example.\n\npub demo(): Option[Unit] ->\n   std.core.Unit.from_string(\"unit\").\n```".

-spec from_string(binary()) -> std_core_option:typer_option(unit).

from_string(Value) ->
    case Value =:= "unit" of
    true -> {'some', unit};
    false -> 'none'
end.

