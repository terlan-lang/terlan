//! Static generated-call identities shared by VM-owned runtime adapters.

use super::ReplValue;

/// One closure-free generated function that a VM adapter may invoke.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmNativeCallableRef {
    /// Module that owns the generated native export.
    pub(crate) module: String,
    /// Generated native export function name.
    pub(crate) function: String,
    /// Exact generated native export arity.
    pub(crate) arity: usize,
}

impl VmNativeCallableRef {
    /// Encodes this identity into the closed runtime value protocol.
    pub(crate) fn into_value(self) -> ReplValue {
        ReplValue::Tuple(vec![
            ReplValue::Atom("$tvm_native_callable".to_string()),
            ReplValue::String(self.module),
            ReplValue::String(self.function),
            ReplValue::Int(i64::try_from(self.arity).expect("callable arity fits i64")),
        ])
    }

    /// Decodes one identity from the closed runtime value protocol.
    pub(crate) fn from_value(value: &ReplValue) -> Option<Self> {
        let ReplValue::Tuple(fields) = value else {
            return None;
        };
        let [ReplValue::Atom(tag), ReplValue::String(module), ReplValue::String(function), ReplValue::Int(arity)] =
            fields.as_slice()
        else {
            return None;
        };
        if !matches!(tag.as_str(), "$tvm_native_callable" | "$tvm_http_callable") || *arity < 0 {
            return None;
        }
        Some(Self {
            module: module.clone(),
            function: function.clone(),
            arity: usize::try_from(*arity).ok()?,
        })
    }
}
