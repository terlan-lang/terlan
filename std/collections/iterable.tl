/**
 * Core iterable trait.
 *
 * `std.collections.Iterable` is the portable contract used by collection traversal.
 */

module std.collections.Iterable.

import std.collections.Iterator.{Iterator}.
import std.collections.List.
import type std.collections.List.List.
import std.collections.Map.
import type std.collections.Map.Map.
import std.collections.Set.
import type std.collections.Set.Set.

/**
 * Iterable connects a collection type `C` to the item type `T` it yields.
 *
 * Input: collection type `C` and yielded item type `T`.
 * Output: a trait requiring an `iterator(collection)` method.
 * Transformation: exposes traversal as explicit state creation instead of
 * depending on a platform-native iterator protocol.
 *
 * Release item shapes:
 * - `List[T]` yields `T`.
 * - `Set[T]` yields `T`.
 * - `Map[K, V]` yields `{K, V}`.
 */
pub trait Iterable[C, T] {
    iterator(collection: C): Iterator[T].
}.

/**
 * Connects the portable List type to Iterable traversal.
 *
 * Input: one `List[T]`.
 * Output: an `Iterator[T]` positioned at the beginning of the list.
 * Transformation: delegates to the compiler-owned `std.collections.List.iterator`
 * receiver contract so trait dispatch and direct receiver dispatch share the
 * same backend-neutral traversal shape.
 */
pub impl Iterable[List[T], T] for List[T] {
    iterator(collection: List[T]): Iterator[T] ->
        List.iterator(collection).
}.

/**
 * Connects the portable Set type to Iterable traversal.
 *
 * Input: one `Set[T]`.
 * Output: an `Iterator[T]` positioned at the beginning of the set values.
 * Transformation: delegates to the compiler-owned `std.collections.Set.iterator`
 * receiver contract so trait dispatch and direct receiver dispatch share the
 * same backend-neutral traversal shape.
 */
pub impl Iterable[Set[T], T] for Set[T] {
    iterator(collection: Set[T]): Iterator[T] ->
        Set.iterator(collection).
}.

/**
 * Connects the portable Map type to Iterable traversal.
 *
 * Input: one `Map[K, V]`.
 * Output: an `Iterator[{K, V}]` positioned at the beginning of the map entries.
 * Transformation: delegates to the compiler-owned `std.collections.Map.iterator`
 * receiver contract so trait dispatch and direct receiver dispatch share the
 * same backend-neutral traversal shape.
 */
pub impl Iterable[Map[K, V], {K, V}] for Map[K, V] {
    iterator(collection: Map[K, V]): Iterator[{K, V}] ->
        Map.iterator(collection).
}.
