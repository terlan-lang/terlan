/**
 * Core enumerable trait.
 *
 * `std.collections.Enumerable` is the portable contract for user-facing
 * collection consumption operations.
 */

module std.collections.Enumerable.

import std.collections.List.
import type std.collections.List.List.
import std.collections.Map.
import type std.collections.Map.Map.
import std.collections.Set.
import type std.collections.Set.Set.
import std.core.Option.{None, Some}.
import std.core.Unit.{Unit}.

/**
 * Enumerable connects a collection type `C` to the item type `T` it contains.
 *
 * Input: collection type `C` and contained item type `T`.
 * Output: a trait requiring effectful consumption and value transformation
 * methods.
 * Transformation: exposes collection behavior through callbacks while leaving
 * traversal-state creation to `std.collections.Iterable`.
 *
 * Release item shapes:
 * - `List[T]` yields `T`.
 * - `Set[T]` yields `T`.
 * - `Map[K, V]` yields `{K, V}`.
 */
pub trait Enumerable[C, T] {
    each(collection: C, cb: (T) -> Unit): Unit.
    map[U](collection: C, cb: (T) -> U): List[U].
    filter(collection: C, predicate: (T) -> Bool): List[T].
    fold[U](collection: C, initial: U, reducer: (U, T) -> U): U.
}.

/**
 * Connects the portable List type to Enumerable consumption.
 *
 * Input: one `List[T]` and one callback from `T` to `Unit`.
 * Output: `Unit` for `each`, or a transformed `List[U]` for `map`.
 * Transformation: delegates to the compiler-owned `std.collections.List.each`
 * receiver contract for consumption and uses list-comprehension source shape
 * for transformation. Trait dispatch and direct receiver dispatch share the
 * same backend-neutral traversal shape.
 */
pub impl Enumerable[List[T], T] for List[T] {
    each(collection: List[T], cb: (T) -> Unit): Unit ->
        List.each(collection, cb).

    map[U](collection: List[T], cb: (T) -> U): List[U] ->
        [cb.(value) | value <- collection].

    filter(collection: List[T], predicate: (T) -> Bool): List[T] ->
        [value | value <- collection, predicate.(value)].

    fold[U](collection: List[T], initial: U, reducer: (U, T) -> U): U ->
        fold_list(collection, initial, reducer).
}.

/**
 * Connects the portable Set type to Enumerable consumption.
 *
 * Input: one `Set[T]` and callbacks over yielded values.
 * Output: `Unit` for `each`, transformed `List[U]` for `map`,
 * filtered `List[T]` for `filter`, and the final accumulator for `fold`.
 * Transformation: delegates traversal to `Set.iterator()` and keeps
 * user-facing transformations in portable list result shapes.
 */
pub impl Enumerable[Set[T], T] for Set[T] {
    each(collection: Set[T], cb: (T) -> Unit): Unit ->
        each_iterator(Set.iterator(collection), cb).

    map[U](collection: Set[T], cb: (T) -> U): List[U] ->
        map_iterator(Set.iterator(collection), cb).

    filter(collection: Set[T], predicate: (T) -> Bool): List[T] ->
        filter_iterator(Set.iterator(collection), predicate).

    fold[U](collection: Set[T], initial: U, reducer: (U, T) -> U): U ->
        fold_iterator(Set.iterator(collection), initial, reducer).
}.

/**
 * Connects the portable Map type to Enumerable consumption.
 *
 * Input: one `Map[K, V]` and callbacks over `{K, V}` entries.
 * Output: `Unit` for `each`, transformed `List[U]` for `map`,
 * filtered `List[{K, V}]` for `filter`, and the final accumulator for `fold`.
 * Transformation: delegates traversal to `Map.iterator()` and observes entries
 * as `{K, V}` tuples, independent of backend map representation.
 */
pub impl Enumerable[Map[K, V], {K, V}] for Map[K, V] {
    each(collection: Map[K, V], cb: ({K, V}) -> Unit): Unit ->
        each_iterator(Map.iterator(collection), cb).

    map[U](collection: Map[K, V], cb: ({K, V}) -> U): List[U] ->
        map_iterator(Map.iterator(collection), cb).

    filter(collection: Map[K, V], predicate: ({K, V}) -> Bool): List[{K, V}] ->
        filter_iterator(Map.iterator(collection), predicate).

    fold[U](collection: Map[K, V], initial: U, reducer: (U, {K, V}) -> U): U ->
        fold_iterator(Map.iterator(collection), initial, reducer).
}.

/**
 * Folds a list with an explicit accumulator.
 *
 * Input: one `List[T]`, an initial accumulator `U`, and a reducer from
 * `(U, T)` to `U`.
 * Output: the final accumulator.
 * Transformation: recursively destructures the list and applies the reducer in
 * source order. The BEAM bridge may lower the public trait call to `lists:foldl`
 * while preserving this accumulator-first reducer contract.
 */
fold_list[T, U](collection: List[T], acc: U, reducer: (U, T) -> U): U ->
    case collection {
        [value | rest] ->
            fold_list(rest, reducer.(acc, value), reducer);
        [] ->
            acc
    }.

/**
 * Applies a callback to each value yielded by an iterator.
 *
 * Input: one iterator state and one callback from `T` to `Unit`.
 * Output: `Unit`.
 * Transformation: repeatedly calls `Iterator.next(iterator)` and invokes the
 * callback for each yielded value.
 */
each_iterator[T](iterator: std.collections.Iterator.Iterator[T], cb: (T) -> Unit): Unit ->
    case std.collections.Iterator.next(iterator) {
        None ->
            Unit;
        Some({value, rest}) ->
            let _done = cb.(value);
            each_iterator(rest, cb)
    }.

/**
 * Maps every value yielded by an iterator into a list.
 *
 * Input: one iterator state and one callback from `T` to `U`.
 * Output: `List[U]`.
 * Transformation: repeatedly calls `Iterator.next(iterator)` and prepends the
 * transformed value to the recursive result.
 */
map_iterator[T, U](iterator: std.collections.Iterator.Iterator[T], cb: (T) -> U): List[U] ->
    case std.collections.Iterator.next(iterator) {
        None ->
            [];
        Some({value, rest}) ->
            [cb.(value) | map_iterator(rest, cb)]
    }.

/**
 * Filters every value yielded by an iterator into a list.
 *
 * Input: one iterator state and one predicate from `T` to `Bool`.
 * Output: `List[T]`.
 * Transformation: repeatedly calls `Iterator.next(iterator)` and keeps only
 * yielded values for which the predicate returns `true`.
 */
filter_iterator[T](iterator: std.collections.Iterator.Iterator[T], predicate: (T) -> Bool): List[T] ->
    case std.collections.Iterator.next(iterator) {
        None ->
            [];
        Some({value, rest}) ->
            case predicate.(value) {
                true ->
                    [value | filter_iterator(rest, predicate)];
                false ->
                    filter_iterator(rest, predicate)
            }
    }.

/**
 * Folds every value yielded by an iterator with an explicit accumulator.
 *
 * Input: one iterator state, an initial accumulator `U`, and a reducer from
 * `(U, T)` to `U`.
 * Output: the final accumulator.
 * Transformation: repeatedly calls `Iterator.next(iterator)` and threads the
 * accumulator through the reducer in traversal order.
 */
fold_iterator[T, U](iterator: std.collections.Iterator.Iterator[T], acc: U, reducer: (U, T) -> U): U ->
    case std.collections.Iterator.next(iterator) {
        None ->
            acc;
        Some({value, rest}) ->
            fold_iterator(rest, reducer.(acc, value), reducer)
    }.
