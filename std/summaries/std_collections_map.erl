-module(std_collections_map).

-moduledoc "Core map collection contract.\n\n`std.collections.Map` is the portable key-value collection surface. Backends own\nthe efficient runtime representation while preserving this source contract.".

-export([clear/1, contains_key/2, get/2, is_empty/1, iterator/1, new/0, put/3, remove/2, size/1]).

-export_type([map/2]).

-doc "Map represents a collection of values addressed by keys.\n\nInput: type parameters `K` and `V`.\nOutput: an opaque key-value collection type.\nTransformation: hides the selected backend representation behind the\nportable `std.collections.Map` API.".

-opaque map(_K, _V) :: term().

-doc "Creates an empty map.\n\nInput: no value input.\nOutput: an empty `Map[K, V]`.\nTransformation: lowers through the compiler-owned `core.map.new`\nintrinsic, using the selected backend's canonical empty-map value.".

new() ->
    #{}.

-doc "Returns whether a map contains no entries.\n\nInput: one `Map[K, V]`.\nOutput: `true` when the map has no entries; otherwise `false`.\nTransformation: lowers through the compiler-owned `core.map.is_empty`\nintrinsic and observes the receiver without changing its contents.".

-spec is_empty(map()) -> boolean().

is_empty(Map) ->
    maps:size(Map) =:= 0.

-doc "Returns the number of entries in a map.\n\nInput: one `Map[K, V]`.\nOutput: the number of key-value entries currently visible in the map.\nTransformation: lowers through the compiler-owned `core.map.size`\nintrinsic and observes the receiver without changing its contents.".

-spec size(map()) -> integer().

size(Map) ->
    maps:size(Map).

-doc "Looks up a value by key.\n\nInput: one `Map[K, V]` receiver and one key of type `K`.\nOutput: `Some(value)` when the key exists; otherwise `None`.\nTransformation: lowers through the compiler-owned `core.map.get`\nintrinsic and observes the receiver without changing its contents.".

-spec get(map(), _K) -> std_core_option:typer_option(_V).

get(Map, Key) ->
    case maps:find(Key, Map) of
    {ok, Value} -> {'some', Value};
    error -> 'none'
end.

-doc "Returns whether a key exists.\n\nInput: one `Map[K, V]` receiver and one key of type `K`.\nOutput: `true` when the key exists; otherwise `false`.\nTransformation: lowers through the compiler-owned `core.map.contains_key`\nintrinsic and observes the receiver without changing its contents.".

-spec contains_key(map(), _K) -> boolean().

contains_key(Map, Key) ->
    maps:is_key(Key, Map).

-doc "Creates an iterator over key-value entries.\n\nInput: one `Map[K, V]`.\nOutput: an `Iterator[{K, V}]` yielding one `{key, value}` pair per entry.\nTransformation: lowers through the compiler-owned `core.map.iterator`\nintrinsic. Backends choose their own traversal order and representation while\npreserving the portable pair-yielding contract.".

-spec iterator(map()) -> std_collections_iterator:iterator({_K, _V}).

iterator(Map) ->
    maps:to_list(Map).

-doc "Inserts or replaces a key-value entry.\n\nInput: a mutable `Map[K, V]` receiver, one key, and one value.\nOutput: `Unit`.\nTransformation: lowers through the compiler-owned `core.map.put`\nintrinsic and updates the receiver binding in source order. Pipe chains\ncontinue with the updated receiver.".

-spec put(map(), _K, _V) -> map().

put(Map, Key, Value) ->
    maps:put(Key, Value, Map).

-doc "Removes a key-value entry if it exists.\n\nInput: a mutable `Map[K, V]` receiver and one key.\nOutput: `Unit`.\nTransformation: lowers through the compiler-owned `core.map.remove`\nintrinsic and updates the receiver binding in source order.".

-spec remove(map(), _K) -> map().

remove(Map, Key) ->
    maps:remove(Key, Map).

-doc "Removes every entry from a map.\n\nInput: a mutable `Map[K, V]` receiver.\nOutput: `Unit`.\nTransformation: lowers through the compiler-owned `core.map.clear`\nintrinsic and updates the receiver binding so it becomes empty.".

-spec clear(map()) -> map().

clear(Map) ->
    #{}.

