pub(super) use super::*;
pub(super) use crate::formal_pipeline::compile_syntax_module_through_phases_with_profile;
pub(super) use crate::validation::native_policy::NativePolicy;
pub(super) use crate::validation::target_profile::TargetProfile;
pub(super) use crate::DiagnosticFormat;

#[cfg(test)]
#[path = "emit_js_test/direct_core_lowering.rs"]
mod direct_core_lowering;
#[cfg(test)]
#[path = "emit_js_test/fallback_lowering.rs"]
mod fallback_lowering;
#[cfg(test)]
#[path = "emit_js_test/intrinsics_and_declarations.rs"]
mod intrinsics_and_declarations;
