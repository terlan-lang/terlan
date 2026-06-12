//! Core identity value type.
//!
//! `std.core.Identity` wraps one value without adding effects.

module std.core.Identity.

/// Identity represents one unchanged value.
///
/// It is useful as the simplest value context for APIs that are generic over
/// a container-like type.
///
/// ```terlan
/// module identity_example.
///
/// pub type Identity[T] =
///     {Atom["identity"], value: T}.
///
/// pub wrap(value: Int): Identity[Int] ->
///     Identity(value).
/// ```
pub type Identity[T] =
    {Atom["identity"], value: T}.

/// Wraps a value in `Identity`.
///
/// ```terlan
/// module identity_constructor_example.
///
/// pub type Identity[T] =
///     {Atom["identity"], value: T}.
///
/// pub identity(value: T): Identity[T] ->
///     Identity(value).
/// ```
pub identity(value: T): Identity[T] ->
    Identity(value).

/// Extracts the value from `Identity`.
///
/// ```terlan
/// module identity_value_example.
///
/// pub type Identity[T] =
///     {Atom["identity"], value: T}.
///
/// pub value(value: Identity[T]): T.
///
/// value(Identity(inner)) ->
///     inner.
/// ```
pub value(value: Identity[T]): T.

value(Identity(inner)) ->
    inner.
