//! Typed declarations for value-only native package functions.

/// Declares an exact, non-coercing argument contract without manual indexing.
///
/// The declared argument list supplies both catalog arity and runtime validation.
/// All arguments are converted before the body runs. The body returns a
/// `Result<T, BoundaryError>` and `T` must convert into an owned `NativeValue`.
/// Borrowed input is valid only for the duration of this synchronous call.
/// This does not make blocking work scheduler-safe or grant resource authority.
///
/// ```
/// use terlan_runtime_abi::{native_binding, NativeBinding, NativeValue};
/// const LENGTH: NativeBinding = native_binding!("example.length", |text: &str| {
///     Ok(text.len() as i64)
/// });
/// assert_eq!(LENGTH.arity, 1);
/// assert_eq!((LENGTH.invoke)(&["hello".into()]).unwrap(), NativeValue::Int(5));
/// ```
///
/// A semicolon separates an explicitly authorized mutable host context from
/// the value arguments. Context authority and scheduling remain host-owned.
///
/// ```
/// use terlan_runtime_abi::{native_binding, NativeContextBinding, NativeValue};
/// const APPEND: NativeContextBinding<String> = native_binding!(
///     "example.append", |text: &mut String; suffix: &str| {
///         text.push_str(suffix);
///         Ok(())
///     }
/// );
/// let mut text = String::from("hello");
/// assert_eq!(APPEND.call(&mut text, &["!".into()]).unwrap(), NativeValue::Unit);
/// assert_eq!(text, "hello!");
/// ```
///
/// Duplicate argument names, unsupported types, and
/// captured ambient state are rejected by Rust rather than silently remapped.
///
/// ```compile_fail
/// use terlan_runtime_abi::native_binding;
/// let _ = native_binding!("duplicate", |value: i64, value: i64| { Ok(value) });
/// ```
/// ```compile_fail
/// use terlan_runtime_abi::native_binding;
/// let _ = native_binding!("pointer", |value: *const u8| { Ok(()) });
/// ```
/// ```compile_fail
/// use terlan_runtime_abi::native_binding;
/// let captured = String::from("ambient");
/// let _ = native_binding!("capture", || { Ok(captured.clone()) });
/// ```
#[macro_export]
macro_rules! native_binding {
    ($operation:literal, |$context:ident : $context_type:ty; $($argument:ident : $ty:ty),* $(,)?| $body:block) => {
        $crate::NativeContextBinding::new(
            $operation,
            <[&str]>::len(&[$(stringify!($argument)),*]),
            |$context: $context_type, __native_arguments| {
                $crate::native_binding!(@invoke $operation, __native_arguments, |$($argument: $ty),*| $body)
            },
        )
    };
    ($operation:literal, |$($argument:ident : $ty:ty),* $(,)?| $body:block) => {
        $crate::NativeBinding {
            operation: $operation,
            arity: <[&str]>::len(&[$(stringify!($argument)),*]),
            invoke: |__native_arguments| {
                $crate::native_binding!(@invoke $operation, __native_arguments, |$($argument: $ty),*| $body)
            },
        }
    };
    ($operation:literal, || $body:block) => {
        $crate::native_binding!($operation, | | $body)
    };
    (@invoke $operation:literal, $args:expr, |$($argument:ident : $ty:ty),*| $body:block) => {{
        let [$($argument),*] = $crate::NativeBinding::arguments::<{
            <[&str]>::len(&[$(stringify!($argument)),*])
        }>($operation, $args)?;
        $(let $argument = <$ty as $crate::FromNativeValue>::from_native($argument)?;)*
        let __native_result: ::core::result::Result<_, $crate::BoundaryError> = $body;
        __native_result.map($crate::NativeValue::from)
    }};
}

#[cfg(test)]
#[path = "native_binding_test.rs"]
mod tests;
