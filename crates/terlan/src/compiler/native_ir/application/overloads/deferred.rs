//! Defers generic trait choices until concrete instantiation.

use super::*;

/// Assigns unique internal identities to typed overloads and rewrites calls.
///
/// Source interfaces remain keyed by their public Terlan name. The native
/// application closure instead receives a distinct name for every parameter
/// vector so its compact `(name, arity)` resolver cannot merge overloads.
#[derive(Debug)]
pub(in crate::compiler::native_ir::application) struct DeferredOverloads(
    HashMap<OverloadKey, Vec<OverloadCandidate>>,
);

impl DeferredOverloads {
    /// Instantiated bodies now have concrete types and retain caller visibility.
    pub(in crate::compiler::native_ir::application) fn resolve(
        self,
        cores: &mut [CoreModule],
    ) -> NativeIrResult<()> {
        rewrite_application(cores, &self.0)
    }
}

pub(in crate::compiler::native_ir::application) fn resolve_typed_overloads(
    cores: &mut [CoreModule],
) -> NativeIrResult<DeferredOverloads> {
    let mut groups = collect_overload_groups(cores)?;
    rename_overload_declarations(cores, &groups)?;
    trait_methods::collect(cores, &mut groups);
    rewrite_application(cores, &groups)?;
    Ok(DeferredOverloads(groups))
}
