//! Declaration-driven projections of maintained native APIs into owned records.

/// Copies selected shared-receiver accessors into an owned native record.
///
/// Field names are exactly the accessor names. Rust checks each method call and
/// its `Into<NativeValue>` conversion; this macro permits neither renaming nor
/// arbitrary mapping expressions. The receiver expression is evaluated once.
/// Borrowed outputs are copied before the receiver is released.
///
/// ```
/// use terlan_runtime_abi::{native_record, NativeValue};
/// struct Message(String);
/// impl Message {
///     fn text(&self) -> &str { &self.0 }
///     fn label(&self) -> Option<&str> { None }
/// }
/// let record = native_record!(Message, Message("hello".into()), { text, label });
/// assert!(matches!(record, NativeValue::Record { name, fields }
///     if name == "Message" && fields[0].0 == "text" && fields[1].0 == "label"));
/// ```
///
/// Already-owned fields can be moved without repeating their names:
///
/// ```
/// use terlan_runtime_abi::{native_record, NativeValue};
/// let text = String::from("hello");
/// let entries = NativeValue::Map(vec![]);
/// let record = native_record!(Message, { text, entries });
/// assert!(matches!(record, NativeValue::Record { name, .. } if name == "Message"));
/// ```
///
/// ```compile_fail
/// use terlan_runtime_abi::native_record;
/// let text = "hello";
/// let _ = native_record!(Text, { text, text });
/// ```
///
/// Missing upstream accessors, duplicate fields, unsupported values, and field
/// aliases fail compilation rather than silently producing a different shape.
///
/// ```compile_fail
/// use terlan_runtime_abi::native_record;
/// let _ = native_record!(Text, "hello", { missing });
/// ```
/// ```compile_fail
/// use terlan_runtime_abi::native_record;
/// let _ = native_record!(Text, String::from("hello"), { as_str, as_str });
/// ```
/// ```compile_fail
/// use terlan_runtime_abi::native_record;
/// let _ = native_record!(Text, String::from("hello"), { as_ptr });
/// ```
/// ```compile_fail
/// use terlan_runtime_abi::native_record;
/// let _ = native_record!(Text, String::from("hello"), { text: as_str });
/// ```
#[macro_export]
macro_rules! native_record {
    ($name:ident, { $($field:ident),+ $(,)? }) => {{
        let ($($field,)+) = ($($field,)+);
        $crate::NativeValue::Record {
            name: stringify!($name).into(),
            fields: vec![$((stringify!($field).into(), $crate::NativeValue::from($field)),)+],
        }
    }};
    ($name:ident, $value:expr, { $($accessor:ident),+ $(,)? }) => {{
        let __native_record_source = &$value;
        // A tuple pattern also makes duplicate accessor names a compile error.
        let ($($accessor,)+) = ($(__native_record_source.$accessor(),)+);
        $crate::NativeValue::Record {
            name: stringify!($name).into(),
            fields: vec![$((stringify!($accessor).into(), $crate::NativeValue::from($accessor)),)+],
        }
    }};
}

#[cfg(test)]
#[path = "native_record_test.rs"]
mod tests;
