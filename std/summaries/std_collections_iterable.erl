-module(std_collections_iterable).

-moduledoc "Core iterable trait.\n\n`std.collections.Iterable` is the portable contract used by collection traversal.".

-export([typer_trait_iterable_iterator_list_t__dict/2, typer_trait_iterable_iterator_map_k___v__dict/2, typer_trait_iterable_iterator_set_t__dict/2]).

%% trait Iterable.
-doc "Connects the portable List type to Iterable traversal.\n\nInput: one `List[T]`.\nOutput: an `Iterator[T]` positioned at the beginning of the list.\nTransformation: delegates to the compiler-owned `std.collections.List.iterator`\nreceiver contract so trait dispatch and direct receiver dispatch share the\nsame backend-neutral traversal shape.".

-spec typer_trait_iterable_iterator_list_t__dict(map(), std_collections_list:list(T)) -> std_collections_iterator:iterator(T).

typer_trait_iterable_iterator_list_t__dict(_TraitDict, Collection) ->
    Collection.

-doc "Connects the portable Map type to Iterable traversal.\n\nInput: one `Map[K, V]`.\nOutput: an `Iterator[{K, V}]` yielding key-value pairs.\nTransformation: delegates to the compiler-owned `std.collections.Map.iterator`\nreceiver contract so trait dispatch and direct receiver dispatch share the\nsame backend-neutral traversal shape.".

-spec typer_trait_iterable_iterator_map_k___v__dict(map(), std_collections_map:map(K, V)) -> std_collections_iterator:iterator({K, V}).

typer_trait_iterable_iterator_map_k___v__dict(_TraitDict, Collection) ->
    maps:to_list(Collection).

-doc "Connects the portable Set type to Iterable traversal.\n\nInput: one `Set[T]`.\nOutput: an `Iterator[T]` yielding each unique value.\nTransformation: delegates to the compiler-owned `std.collections.Set.iterator`\nreceiver contract so trait dispatch and direct receiver dispatch share the\nsame backend-neutral traversal shape.".

-spec typer_trait_iterable_iterator_set_t__dict(map(), std_collections_set:set(T)) -> std_collections_iterator:iterator(T).

typer_trait_iterable_iterator_set_t__dict(_TraitDict, Collection) ->
    maps:keys(Collection).

