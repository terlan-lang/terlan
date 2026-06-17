-module(std_core_option).

-moduledoc "Core optional value type.\n\n`std.core.Option` represents either no value or one present value.".

-export([and_then/2, compare/3, is_none/1, is_some/1, map/2, with_default/2]).

-export_type([none/0, some/1, typer_option/1]).

-doc "None represents a missing optional value.\n\n`None` is a typed alias for the language-neutral atom primitive\n`Atom[\"none\"]`.\n\n```terlan\nmodule none_type_example.\n\npub type None =\n   Atom[\"none\"].\n\npub missing(): None ->\n   None.\n```".

-type none() :: 'none'.

-doc "Some represents a present optional value.\n\n`Some[T]` is a typed alias for the tuple `{Atom[\"some\"], value}`.\n\n```terlan\nmodule some_type_example.\n\npub type Some[T] =\n   {Atom[\"some\"], value: T}.\n\npub present(value: Int): Some[Int] ->\n   Some(value).\n```".

-type some(T) :: {'some', T}.

-doc "Option represents an optional value.\n\nUse `None` when no value is available and `Some(value)` when a value is\npresent.\n\n```terlan\nmodule option_example.\n\npub type None =\n   Atom[\"none\"].\n\npub type Some[T] =\n   {Atom[\"some\"], value: T}.\n\npub type Option[T] =\n     None\n   | Some[T].\n\npub missing(): Option[Int] ->\n   None.\n\npub present(x: Int): Option[Int] ->\n   Some(x).\n```".

-type typer_option(T) :: none() | some(T).

-doc "Returns whether an optional value is present.\n\nInput: one `Option[A]` value.\nOutput: `true` for `Some(value)` and `false` for `None`.\nTransformation: inspects the option shape without changing the contained\nvalue.\n\n```terlan\nmodule option_is_some_example.\n\npub demo(value: Option[Int]): Bool ->\n   std.core.Option.is_some(value).\n```".

-spec is_some(std_core_option:typer_option(_A)) -> boolean().

is_some(Value) ->
    case Value of
    'none' -> false;
    {'some', _} -> true
end.

-doc "Returns whether an optional value is missing.\n\nInput: one `Option[A]` value.\nOutput: `true` for `None` and `false` for `Some(value)`.\nTransformation: inspects the option shape without changing the contained\nvalue.\n\n```terlan\nmodule option_is_none_example.\n\npub demo(value: Option[Int]): Bool ->\n   std.core.Option.is_none(value).\n```".

-spec is_none(std_core_option:typer_option(_A)) -> boolean().

is_none(Value) ->
    case Value of
    'none' -> true;
    {'some', _} -> false
end.

-doc "Maps a present value and leaves `None` unchanged.\n\n```terlan\nmodule option_map_example.\n\npub type None =\n   Atom[\"none\"].\n\npub type Some[T] =\n   {Atom[\"some\"], value: T}.\n\npub type Option[T] =\n     None\n   | Some[T].\n\npub map(value: Option[A], f: (A) -> B): Option[B] ->\n   case value {\n       None ->\n           None;\n\n       Some(x) ->\n           Some(f(x))\n   }.\n\npub demo(value: Option[Int], f: (Int) -> Int): Option[Int] ->\n   map(value, f).\n```".

-spec map(std_core_option:typer_option(A), fun((A) -> B)) -> std_core_option:typer_option(B).

map(Value, F) ->
    case Value of
    'none' -> 'none';
    {'some', X} -> {'some', F(X)}
end.

-doc "Chains a present value into another optional computation.\n\n`and_then` returns `None` when the input is missing. When the input is\npresent, it calls the supplied function and returns that function's\noptional result directly.\n\n```terlan\nmodule option_and_then_example.\n\npub type None = Atom[\"none\"].\n\npub type Some[T] = {Atom[\"some\"], value: T}.\n\npub type Option[T] =\n     None\n   | Some[T].\n\npub and_then(value: Option[A], f: (A) -> Option[B]): Option[B] ->\n   case value {\n       None ->\n           None;\n\n       Some(x) ->\n           f(x)\n   }.\n\npub demo(value: Option[Int], f: (Int) -> Option[Int]): Option[Int] ->\n   and_then(value, f).\n```".

-spec and_then(std_core_option:typer_option(A), fun((A) -> std_core_option:typer_option(B))) -> std_core_option:typer_option(B).

and_then(Value, F) ->
    case Value of
    'none' -> 'none';
    {'some', X} -> F(X)
end.

-doc "Compares two optional values using `None < Some(_)`.\n\nInput: two `Option[A]` values and a comparator for present `A` values.\nOutput: `Lt` when `left` is ordered before `right`, `Eq` when they are\nequal, and `Gt` when `left` is ordered after `right`.\nTransformation: orders missing values before present values and delegates\npresent-value comparison to `value_compare`.\n\n```terlan\nmodule option_compare_example.\n\nimport type std.core.Ordering.Comparison.\n\npub compare_int(left: Int, right: Int): Comparison ->\n   std.core.Int.compare(left, right).\n\npub demo(): Comparison ->\n   std.core.Option.compare(None, Some(1), compare_int).\n```".

-spec compare(std_core_option:typer_option(A), std_core_option:typer_option(A), fun((A, A) -> std_core_ordering:comparison())) -> std_core_ordering:comparison().

compare(Left, Right, Value_compare) ->
    case Left of
    'none' -> case Right of
    'none' -> 'eq';
    {'some', _} -> 'lt'
end;
    {'some', Left_value} -> case Right of
    'none' -> 'gt';
    {'some', Right_value} -> Value_compare(Left_value, Right_value)
end
end.

-doc "Extracts a present value or returns the provided default.\n\n```terlan\nmodule option_with_default_example.\n\npub type None =\n   Atom[\"none\"].\n\npub type Some[T] =\n   {Atom[\"some\"], value: T}.\n\npub type Option[T] =\n     None\n   | Some[T].\n\npub with_default(value: Option[A], default: A): A ->\n   case value {\n       None ->\n           default;\n\n       Some(x) ->\n           x\n   }.\n\npub demo(value: Option[Int]): Int ->\n   with_default(value, 0).\n```".

-spec with_default(std_core_option:typer_option(A), A) -> A.

with_default(Value, Default) ->
    case Value of
    'none' -> Default;
    {'some', X} -> X
end.

