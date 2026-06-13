/**
 * Core map collection contract.
 *
 * `std.collections.Map` is the portable key-value collection surface. Backends own
 * the efficient runtime representation while preserving this source contract.
 */

module std.collections.Map.

import std.collections.Iterator.{Iterator}.
import type std.core.Option.Option.
import std.core.Unit.{Unit}.

/**
 * Map represents a collection of values addressed by keys.
 *
 * Input: type parameters `K` and `V`.
 * Output: an opaque key-value collection type.
 * Transformation: hides the selected backend representation behind the
 * portable `std.collections.Map` API.
 */
pub opaque type Map[K, V].

/**
 * Creates an empty map.
 *
 * Input: no value input.
 * Output: an empty `Map[K, V]`.
 * Transformation: lowers through the compiler-owned `core.map.new`
 * intrinsic, using the selected backend's canonical empty-map value.
 */
@compiler.intrinsic {core.map.new}
pub new[K, V](): Map[K, V] ->
    {}.

/**
 * Returns whether a map contains no entries.
 *
 * Input: one `Map[K, V]`.
 * Output: `true` when the map has no entries; otherwise `false`.
 * Transformation: lowers through the compiler-owned `core.map.is_empty`
 * intrinsic and observes the receiver without changing its contents.
 */
@compiler.intrinsic {core.map.is_empty}
pub (map: Map[K, V]) is_empty(): Bool ->
    true.

/**
 * Returns the number of entries in a map.
 *
 * Input: one `Map[K, V]`.
 * Output: the number of key-value entries currently visible in the map.
 * Transformation: lowers through the compiler-owned `core.map.size`
 * intrinsic and observes the receiver without changing its contents.
 */
@compiler.intrinsic {core.map.size}
pub (map: Map[K, V]) size(): Int ->
    0.

/**
 * Looks up a value by key.
 *
 * Input: one `Map[K, V]` receiver and one key of type `K`.
 * Output: `Some(value)` when the key exists; otherwise `None`.
 * Transformation: lowers through the compiler-owned `core.map.get`
 * intrinsic and observes the receiver without changing its contents.
 */
@compiler.intrinsic {core.map.get}
pub (map: Map[K, V]) get(key: K): Option[V] ->
    None.

/**
 * Returns whether a key exists.
 *
 * Input: one `Map[K, V]` receiver and one key of type `K`.
 * Output: `true` when the key exists; otherwise `false`.
 * Transformation: lowers through the compiler-owned `core.map.contains_key`
 * intrinsic and observes the receiver without changing its contents.
 */
@compiler.intrinsic {core.map.contains_key}
pub (map: Map[K, V]) contains_key(key: K): Bool ->
    false.

/**
 * Inserts or replaces a key-value entry.
 *
 * Input: a mutable `Map[K, V]` receiver, one key, and one value.
 * Output: `Unit`.
 * Transformation: lowers through the compiler-owned `core.map.put`
 * intrinsic and updates the receiver binding in source order. Pipe chains
 * continue with the updated receiver.
 */
@compiler.intrinsic {core.map.put}
pub (mut map: Map[K, V]) put(key: K, value: V): Unit ->
    map.

/**
 * Removes a key-value entry if it exists.
 *
 * Input: a mutable `Map[K, V]` receiver and one key.
 * Output: `Unit`.
 * Transformation: lowers through the compiler-owned `core.map.remove`
 * intrinsic and updates the receiver binding in source order.
 */
@compiler.intrinsic {core.map.remove}
pub (mut map: Map[K, V]) remove(key: K): Unit ->
    map.

/**
 * Removes every entry from a map.
 *
 * Input: a mutable `Map[K, V]` receiver.
 * Output: `Unit`.
 * Transformation: lowers through the compiler-owned `core.map.clear`
 * intrinsic and updates the receiver binding so it becomes empty.
 */
@compiler.intrinsic {core.map.clear}
pub (mut map: Map[K, V]) clear(): Unit ->
    map.

/**
 * Returns an iterator over key-value entries.
 *
 * Input: one `Map[K, V]`.
 * Output: an `Iterator[{K, V}]` that yields `{key, value}` entries.
 * Transformation: lowers through the compiler-owned `core.map.iterator`
 * intrinsic. Backends choose their native map traversal representation, but
 * portable Terlan observes entries as `{K, V}` tuples.
 */
@compiler.intrinsic {core.map.iterator}
pub (map: Map[K, V]) iterator(): Iterator[{K, V}] ->
    [].
