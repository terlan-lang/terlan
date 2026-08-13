//! Complete application-object helper shared by NativeIR tests.

use super::super::{NativeCodegenPolicy, NativeModule};

pub(crate) fn emit_native_application_object(
    application: &str,
    natives: &[NativeModule],
) -> Result<Vec<u8>, String> {
    Ok(super::emit_native_application_object_with_policy(
        application,
        natives,
        NativeCodegenPolicy::Development,
    )?)
}
