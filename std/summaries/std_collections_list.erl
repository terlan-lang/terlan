-module(std_collections_list).

-moduledoc "Core list collection contract.\n\n`std.collections.List` is the portable ordered collection surface for Terlan list\nvalues.".

-export([clear/1, each/2, first/1, is_empty/1, iterator/1, length/1, new/0, push/2]).

-export_type([list/1]).

-doc "List represents an ordered collection of values.\n\nInput: type parameter `T`.\nOutput: an opaque ordered collection type.\nTransformation: hides the selected backend representation behind the\nportable `std.collections.List` API.".

-opaque list(_T) :: term().

-doc "Creates an empty list.\n\nInput: no value input.\nOutput: an empty `List[T]`.\nTransformation: lowers through the compiler-owned `core.list.new`\nintrinsic, using the selected backend's canonical empty-list value.".

new() ->
    [].

-doc "Returns whether a list contains no values.\n\nInput: one `List[T]`.\nOutput: `true` when the list is empty; otherwise `false`.\nTransformation: lowers through the compiler-owned `core.list.is_empty`\nintrinsic and observes the receiver without changing its contents.".

-spec is_empty([_T]) -> boolean().

is_empty(List) ->
    List =:= [].

-doc "Returns the number of values in a list.\n\nInput: one `List[T]`.\nOutput: the number of values currently visible in the list.\nTransformation: lowers through the compiler-owned `core.list.length`\nintrinsic and observes the receiver without changing its contents.".

-spec length([_T]) -> integer().

length(List) ->
    erlang:length(List).

-doc "Returns the first value in a list.\n\nInput: one `List[T]`.\nOutput: `Some(value)` when the list is non-empty; otherwise `None`.\nTransformation: lowers through the compiler-owned `core.list.first`\nintrinsic and observes the receiver without changing its contents.".

-spec first([T]) -> std_core_option:typer_option(T).

first(List) ->
    case List of
    [Head|_] -> {'some', Head};
    [] -> 'none'
end.

-doc "Returns an iterator over the list.\n\nInput: one `List[T]`.\nOutput: an `Iterator[T]` positioned at the beginning of the list.\nTransformation: lowers through the compiler-owned `core.list.iterator`\nintrinsic. The source-level iterator state is immutable; backends choose\nthe efficient representation behind the opaque iterator type.".

-spec iterator([T]) -> std_collections_iterator:iterator(T).

iterator(List) ->
    List.

-doc "Applies a callback to each value in a list.\n\nInput: one `List[T]` receiver and one callback from `T` to `Unit`.\nOutput: `Unit`.\nTransformation: creates a traversal state with `list.iterator()` and\ndelegates to `std.collections.Iterator.each`, preserving list contents while the\ncallback performs any requested effects.".

-spec each([T], fun((T) -> unit)) -> std_core_unit:unit().

each(List, Cb) ->
    std_collections_iterator:each(List, Cb).

-doc "Appends a value to the end of a list.\n\nInput: a mutable `List[T]` receiver and one value of type `T`.\nOutput: `Unit`.\nTransformation: lowers through the compiler-owned `core.list.push`\nintrinsic and updates the receiver binding in source order. Pipe chains\ncontinue with the updated receiver.".

-spec push([T], T) -> [T].

push(List, Value) ->
    lists:append(List, [Value]).

-doc "Removes every value from a list.\n\nInput: a mutable `List[T]` receiver.\nOutput: `Unit`.\nTransformation: lowers through the compiler-owned `core.list.clear`\nintrinsic and updates the receiver binding so it becomes empty.".

-spec clear([T]) -> [T].

clear(List) ->
    [].

