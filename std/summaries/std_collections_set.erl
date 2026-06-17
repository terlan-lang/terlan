-module(std_collections_set).

-moduledoc "Core set collection contract.\n\n`std.collections.Set` is the portable unique-value collection surface.".

-export([add/2, clear/1, contains/2, is_empty/1, iterator/1, new/0, remove/2, size/1]).

-export_type([set/1]).

-doc "Set represents a collection of unique values.\n\nInput: type parameter `T`.\nOutput: an opaque unique-value collection type.\nTransformation: hides the selected backend representation behind the\nportable `std.collections.Set` API.".

-opaque set(_T) :: term().

-doc "Creates an empty set.\n\nInput: no value input.\nOutput: an empty `Set[T]`.\nTransformation: lowers through the compiler-owned `core.set.new`\nintrinsic, using the selected backend's canonical empty-set value.".

new() ->
    #{}.

-doc "Returns whether a set contains no values.\n\nInput: one `Set[T]`.\nOutput: `true` when the set is empty; otherwise `false`.\nTransformation: lowers through the compiler-owned `core.set.is_empty`\nintrinsic and observes the receiver without changing its contents.".

-spec is_empty(map()) -> boolean().

is_empty(Set) ->
    maps:size(Set) =:= 0.

-doc "Returns the number of unique values in a set.\n\nInput: one `Set[T]`.\nOutput: the number of unique values currently visible in the set.\nTransformation: lowers through the compiler-owned `core.set.size`\nintrinsic and observes the receiver without changing its contents.".

-spec size(map()) -> integer().

size(Set) ->
    maps:size(Set).

-doc "Returns whether a value exists in a set.\n\nInput: one `Set[T]` receiver and one value of type `T`.\nOutput: `true` when the value exists; otherwise `false`.\nTransformation: lowers through the compiler-owned `core.set.contains`\nintrinsic and observes the receiver without changing its contents.".

-spec contains(map(), _T) -> boolean().

contains(Set, Value) ->
    maps:is_key(Value, Set).

-doc "Creates an iterator over set values.\n\nInput: one `Set[T]`.\nOutput: an `Iterator[T]` yielding each unique value.\nTransformation: lowers through the compiler-owned `core.set.iterator`\nintrinsic. Backends choose their own traversal order and representation while\npreserving the portable value-yielding contract.".

-spec iterator(map()) -> std_collections_iterator:iterator(_T).

iterator(Set) ->
    maps:keys(Set).

-doc "Adds a value to a set.\n\nInput: a mutable `Set[T]` receiver and one value of type `T`.\nOutput: `Unit`.\nTransformation: lowers through the compiler-owned `core.set.add`\nintrinsic and updates the receiver binding in source order. Pipe chains\ncontinue with the updated receiver.".

-spec add(map(), _T) -> map().

add(Set, Value) ->
    maps:put(Value, true, Set).

-doc "Removes a value from a set.\n\nInput: a mutable `Set[T]` receiver and one value of type `T`.\nOutput: `Unit`.\nTransformation: lowers through the compiler-owned `core.set.remove`\nintrinsic and updates the receiver binding in source order.".

-spec remove(map(), _T) -> map().

remove(Set, Value) ->
    maps:remove(Value, Set).

-doc "Removes every value from a set.\n\nInput: a mutable `Set[T]` receiver.\nOutput: `Unit`.\nTransformation: lowers through the compiler-owned `core.set.clear`\nintrinsic and updates the receiver binding so it becomes empty.".

-spec clear(map()) -> map().

clear(Set) ->
    #{}.

