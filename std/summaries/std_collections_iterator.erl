-module(std_collections_iterator).

-moduledoc "Core iterator contract.\n\n`std.collections.Iterator` defines the portable state-passing shape used by\ncollection traversal.".

-export([each/2, next/1]).

-export_type([iterator/1, step/1]).

-doc "Iterator represents a traversal state for values of type `T`.\n\nInput: type parameter `T`.\nOutput: an opaque traversal-state type.\nTransformation: hides the selected backend representation behind the\nportable `std.collections.Iterator` API.".

-opaque iterator(_T) :: term().

-doc "Step represents one yielded value and the next iterator state.\n\nInput: type parameter `T`.\nOutput: a structural pair of the yielded value and next iterator state.\nTransformation: makes traversal state explicit and immutable at the source\nlevel.".

-type step(T) :: {T, iterator(T)}.

-doc "Returns the next value and iterator state, or `None` when exhausted.\n\nInput: one iterator state.\nOutput: `Some({value, next})` for the next item, or `None` when traversal\nis complete.\nTransformation: lowers through the compiler-owned `core.iterator.next`\nintrinsic and advances traversal by returning a new iterator state rather\nthan mutating the current value.".

-spec next(iterator(T)) -> std_core_option:typer_option(step(T)).

next(Iterator) ->
    case Iterator of
    [_TerlanIteratorValue|_TerlanNextIterator] -> {'some', {_TerlanIteratorValue, _TerlanNextIterator}};
    [] -> 'none'
end.

-doc "Applies a callback to each value yielded by an iterator.\n\nInput: one iterator state and one callback from `T` to `Unit`.\nOutput: `Unit`.\nTransformation: repeatedly calls `next(iterator)`. When traversal is\nexhausted, returns `Unit`. When a value is yielded, calls `cb.(value)` for\nits effect and continues with the returned next iterator state.\n\n```terlan\nmodule iterator_each_example.\n\nimport std.collections.Iterator.{Iterator, Step}.\nimport std.core.Option.{None, Option, Some}.\nimport std.core.Unit.{Unit}.\n\npub each(iterator: Iterator[T], cb: (T) -> Unit): Unit ->\n   case next(iterator) {\n       None ->\n           Unit;\n\n       Some({value, rest}) ->\n           let _done = cb.(value);\n           each(rest, cb)\n   }.\n```".

-spec each(iterator(T), fun((T) -> unit)) -> std_core_unit:unit().

each(Iterator, Cb) ->
    case next(Iterator) of
    'none' -> unit;
    {'some', {Value, Rest}} -> begin
    _done = (Cb)(Value),
    each(Rest, Cb)
end
end.

