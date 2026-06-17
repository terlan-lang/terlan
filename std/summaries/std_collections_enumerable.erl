-module(std_collections_enumerable).

-moduledoc "Core enumerable trait.\n\n`std.collections.Enumerable` is the portable contract for user-facing\ncollection consumption operations.".

-export([typer_trait_enumerable_each_list_t__dict/3, typer_trait_enumerable_each_map_k___v__dict/3, typer_trait_enumerable_each_set_t__dict/3, typer_trait_enumerable_filter_list_t__dict/3, typer_trait_enumerable_filter_map_k___v__dict/3, typer_trait_enumerable_filter_set_t__dict/3, typer_trait_enumerable_fold_list_t__dict/4, typer_trait_enumerable_fold_map_k___v__dict/4, typer_trait_enumerable_fold_set_t__dict/4, typer_trait_enumerable_map_list_t__dict/3, typer_trait_enumerable_map_map_k___v__dict/3, typer_trait_enumerable_map_set_t__dict/3]).

%% trait Enumerable.
-doc "Connects the portable List type to Enumerable consumption.\n\nInput: one `List[T]` and one callback from `T` to `Unit`.\nOutput: `Unit` for `each`, or a transformed `List[U]` for `map`.\nTransformation: delegates to the compiler-owned `std.collections.List.each`\nreceiver contract for consumption and uses list-comprehension source shape\nfor transformation. Trait dispatch and direct receiver dispatch share the\nsame backend-neutral traversal shape.".

-spec typer_trait_enumerable_each_list_t__dict(map(), std_collections_list:list(T), fun((T) -> unit)) -> std_core_unit:unit().

typer_trait_enumerable_each_list_t__dict(_TraitDict, Collection, Cb) ->
    std_collections_list:each(Collection, Cb).

-doc "Connects the portable List type to Enumerable consumption.\n\nInput: one `List[T]` and one callback from `T` to `Unit`.\nOutput: `Unit` for `each`, or a transformed `List[U]` for `map`.\nTransformation: delegates to the compiler-owned `std.collections.List.each`\nreceiver contract for consumption and uses list-comprehension source shape\nfor transformation. Trait dispatch and direct receiver dispatch share the\nsame backend-neutral traversal shape.".

-spec typer_trait_enumerable_map_list_t__dict(map(), std_collections_list:list(T), fun((T) -> U)) -> std_collections_list:list(U).

typer_trait_enumerable_map_list_t__dict(_TraitDict, Collection, Cb) ->
    map_iterator(Collection, Cb).

-doc "Connects the portable List type to Enumerable consumption.\n\nInput: one `List[T]` and one callback from `T` to `Unit`.\nOutput: `Unit` for `each`, or a transformed `List[U]` for `map`.\nTransformation: delegates to the compiler-owned `std.collections.List.each`\nreceiver contract for consumption and uses list-comprehension source shape\nfor transformation. Trait dispatch and direct receiver dispatch share the\nsame backend-neutral traversal shape.".

-spec typer_trait_enumerable_filter_list_t__dict(map(), std_collections_list:list(T), fun((T) -> boolean())) -> std_collections_list:list(T).

typer_trait_enumerable_filter_list_t__dict(_TraitDict, Collection, Predicate) ->
    filter_iterator(Collection, Predicate).

-doc "Connects the portable List type to Enumerable consumption.\n\nInput: one `List[T]` and one callback from `T` to `Unit`.\nOutput: `Unit` for `each`, or a transformed `List[U]` for `map`.\nTransformation: delegates to the compiler-owned `std.collections.List.each`\nreceiver contract for consumption and uses list-comprehension source shape\nfor transformation. Trait dispatch and direct receiver dispatch share the\nsame backend-neutral traversal shape.".

-spec typer_trait_enumerable_fold_list_t__dict(map(), std_collections_list:list(T), U, fun((U, T) -> U)) -> U.

typer_trait_enumerable_fold_list_t__dict(_TraitDict, Collection, Initial, Reducer) ->
    fold_iterator(Collection, Initial, Reducer).

-doc "Connects the portable Map type to Enumerable consumption.\n\nInput: one `Map[K, V]` and callbacks over `{K, V}` pairs.\nOutput: `Unit` for `each`, a transformed `List[U]` for `map`, a filtered\n`List[{K, V}]` for `filter`, or a folded accumulator for `fold`.\nTransformation: delegates traversal to `Map.iterator(collection)` so the map\nrepresentation and traversal order remain backend-owned.".

-spec typer_trait_enumerable_each_map_k___v__dict(map(), std_collections_map:map(K, V), fun(({K, V}) -> unit)) -> std_core_unit:unit().

typer_trait_enumerable_each_map_k___v__dict(_TraitDict, Collection, Cb) ->
    std_collections_iterator:each(maps:to_list(Collection), Cb).

-doc "Connects the portable Map type to Enumerable consumption.\n\nInput: one `Map[K, V]` and callbacks over `{K, V}` pairs.\nOutput: `Unit` for `each`, a transformed `List[U]` for `map`, a filtered\n`List[{K, V}]` for `filter`, or a folded accumulator for `fold`.\nTransformation: delegates traversal to `Map.iterator(collection)` so the map\nrepresentation and traversal order remain backend-owned.".

-spec typer_trait_enumerable_map_map_k___v__dict(map(), std_collections_map:map(K, V), fun(({K, V}) -> U)) -> std_collections_list:list(U).

typer_trait_enumerable_map_map_k___v__dict(_TraitDict, Collection, Cb) ->
    map_iterator(maps:to_list(Collection), Cb).

-doc "Connects the portable Map type to Enumerable consumption.\n\nInput: one `Map[K, V]` and callbacks over `{K, V}` pairs.\nOutput: `Unit` for `each`, a transformed `List[U]` for `map`, a filtered\n`List[{K, V}]` for `filter`, or a folded accumulator for `fold`.\nTransformation: delegates traversal to `Map.iterator(collection)` so the map\nrepresentation and traversal order remain backend-owned.".

-spec typer_trait_enumerable_filter_map_k___v__dict(map(), std_collections_map:map(K, V), fun(({K, V}) -> boolean())) -> std_collections_list:list({K, V}).

typer_trait_enumerable_filter_map_k___v__dict(_TraitDict, Collection, Predicate) ->
    filter_iterator(maps:to_list(Collection), Predicate).

-doc "Connects the portable Map type to Enumerable consumption.\n\nInput: one `Map[K, V]` and callbacks over `{K, V}` pairs.\nOutput: `Unit` for `each`, a transformed `List[U]` for `map`, a filtered\n`List[{K, V}]` for `filter`, or a folded accumulator for `fold`.\nTransformation: delegates traversal to `Map.iterator(collection)` so the map\nrepresentation and traversal order remain backend-owned.".

-spec typer_trait_enumerable_fold_map_k___v__dict(map(), std_collections_map:map(K, V), U, fun((U, {K, V}) -> U)) -> U.

typer_trait_enumerable_fold_map_k___v__dict(_TraitDict, Collection, Initial, Reducer) ->
    fold_iterator(maps:to_list(Collection), Initial, Reducer).

-doc "Connects the portable Set type to Enumerable consumption.\n\nInput: one `Set[T]` and callbacks over `T` values.\nOutput: `Unit` for `each`, a transformed `List[U]` for `map`, a filtered\n`List[T]` for `filter`, or a folded accumulator for `fold`.\nTransformation: delegates traversal to `Set.iterator(collection)` so the set\nrepresentation and traversal order remain backend-owned.".

-spec typer_trait_enumerable_each_set_t__dict(map(), std_collections_set:set(T), fun((T) -> unit)) -> std_core_unit:unit().

typer_trait_enumerable_each_set_t__dict(_TraitDict, Collection, Cb) ->
    std_collections_iterator:each(maps:keys(Collection), Cb).

-doc "Connects the portable Set type to Enumerable consumption.\n\nInput: one `Set[T]` and callbacks over `T` values.\nOutput: `Unit` for `each`, a transformed `List[U]` for `map`, a filtered\n`List[T]` for `filter`, or a folded accumulator for `fold`.\nTransformation: delegates traversal to `Set.iterator(collection)` so the set\nrepresentation and traversal order remain backend-owned.".

-spec typer_trait_enumerable_map_set_t__dict(map(), std_collections_set:set(T), fun((T) -> U)) -> std_collections_list:list(U).

typer_trait_enumerable_map_set_t__dict(_TraitDict, Collection, Cb) ->
    map_iterator(maps:keys(Collection), Cb).

-doc "Connects the portable Set type to Enumerable consumption.\n\nInput: one `Set[T]` and callbacks over `T` values.\nOutput: `Unit` for `each`, a transformed `List[U]` for `map`, a filtered\n`List[T]` for `filter`, or a folded accumulator for `fold`.\nTransformation: delegates traversal to `Set.iterator(collection)` so the set\nrepresentation and traversal order remain backend-owned.".

-spec typer_trait_enumerable_filter_set_t__dict(map(), std_collections_set:set(T), fun((T) -> boolean())) -> std_collections_list:list(T).

typer_trait_enumerable_filter_set_t__dict(_TraitDict, Collection, Predicate) ->
    filter_iterator(maps:keys(Collection), Predicate).

-doc "Connects the portable Set type to Enumerable consumption.\n\nInput: one `Set[T]` and callbacks over `T` values.\nOutput: `Unit` for `each`, a transformed `List[U]` for `map`, a filtered\n`List[T]` for `filter`, or a folded accumulator for `fold`.\nTransformation: delegates traversal to `Set.iterator(collection)` so the set\nrepresentation and traversal order remain backend-owned.".

-spec typer_trait_enumerable_fold_set_t__dict(map(), std_collections_set:set(T), U, fun((U, T) -> U)) -> U.

typer_trait_enumerable_fold_set_t__dict(_TraitDict, Collection, Initial, Reducer) ->
    fold_iterator(maps:keys(Collection), Initial, Reducer).

-doc "Maps iterator values into a list.\n\nInput: one `Iterator[T]` and a callback from `T` to `U`.\nOutput: a `List[U]` containing one transformed value for each yielded item.\nTransformation: recursively consumes explicit iterator state and constructs a\npersistent result list without relying on platform-native iterator protocols.".

-spec map_iterator(std_collections_iterator:iterator(T), fun((T) -> U)) -> std_collections_list:list(U).

map_iterator(Iterator, Cb) ->
    case case Iterator of
    [_TerlanIteratorValue|_TerlanNextIterator] -> {'some', {_TerlanIteratorValue, _TerlanNextIterator}};
    [] -> 'none'
end of
    {'some', {Value, Next}} -> [(Cb)(Value)|map_iterator(Next, Cb)];
    'none' -> []
end.

-doc "Filters iterator values into a list.\n\nInput: one `Iterator[T]` and a predicate from `T` to `Bool`.\nOutput: a `List[T]` containing values whose predicate result is `true`.\nTransformation: recursively consumes explicit iterator state and preserves the\nrelative yield order exposed by the selected backend iterator.".

-spec filter_iterator(std_collections_iterator:iterator(T), fun((T) -> boolean())) -> std_collections_list:list(T).

filter_iterator(Iterator, Predicate) ->
    case case Iterator of
    [_TerlanIteratorValue|_TerlanNextIterator] -> {'some', {_TerlanIteratorValue, _TerlanNextIterator}};
    [] -> 'none'
end of
    {'some', {Value, Next}} -> case (Predicate)(Value) of
    true -> [Value|filter_iterator(Next, Predicate)];
    false -> filter_iterator(Next, Predicate)
end;
    'none' -> []
end.

-doc "Folds iterator values with an explicit accumulator.\n\nInput: one `Iterator[T]`, an initial accumulator `U`, and a reducer from\n`(U, T)` to `U`.\nOutput: the final accumulator.\nTransformation: recursively consumes explicit iterator state and applies the\nreducer in source order using Terlan's accumulator-first callback convention.".

-spec fold_iterator(std_collections_iterator:iterator(T), U, fun((U, T) -> U)) -> U.

fold_iterator(Iterator, Acc, Reducer) ->
    case case Iterator of
    [_TerlanIteratorValue|_TerlanNextIterator] -> {'some', {_TerlanIteratorValue, _TerlanNextIterator}};
    [] -> 'none'
end of
    {'some', {Value, Next}} -> fold_iterator(Next, (Reducer)(Acc, Value), Reducer);
    'none' -> Acc
end.

