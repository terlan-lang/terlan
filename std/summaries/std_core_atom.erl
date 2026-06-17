-module(std_core_atom).

-moduledoc "Core symbolic atom helpers.\n\n`std.core.Atom` documents and supports Terlan's language-neutral singleton\nsymbol primitive. Source code should define named aliases with\n`Atom[\"name\"]` and use those aliases in expressions and patterns.".

-export([equal/2, to_string/1]).

-doc "Returns whether two singleton atom-backed values are equal.\n\nInput: two values of the same singleton atom-backed type `T`.\nOutput: `true` when both values are the same symbolic value; otherwise\n`false`.\nTransformation: delegates to Terlan equality without exposing backend atom\nsyntax or target-specific representation details.\n\n```terlan\nmodule atom_equal_example.\n\npub type Ready = Atom[\"ready\"].\n\npub demo(): Bool ->\n   std.core.Atom.equal(Ready, Ready).\n```".

-spec equal(T, T) -> boolean().

equal(Left, Right) ->
    Left =:= Right.

-doc "Renders a singleton atom-backed value as canonical text.\n\nInput: one language-neutral atom value, including values introduced through\n`Atom[\"name\"]` aliases.\nOutput: the canonical `String` payload for that atom.\nTransformation: lowers through the compiler-owned `core.atom.to_string`\nintrinsic so source code does not depend on a backend atom representation.\n\n```terlan\nmodule atom_to_string_example.\n\npub type Ready = Atom[\"ready\"].\n\npub demo(): String ->\n   std.core.Atom.to_string(Ready).\n```".

-spec to_string(atom()) -> binary().

to_string(Value) ->
    erlang:atom_to_list(Value).

