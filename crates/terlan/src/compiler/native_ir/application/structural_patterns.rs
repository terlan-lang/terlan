//! Layout-independent structural normalization performed before case lowering.

use crate::terlan_typeck::CoreModule;

/// Scalar-replaces tuple destructuring before case normalization turns it into
/// managed matching control flow.
pub(super) fn scalar_replace(cores: &mut [CoreModule]) -> Result<(), super::super::NativeIrError> {
    let layouts = super::super::constructors::native_application_constructor_layouts(cores)?;
    for core in cores {
        let layouts = &layouts[&core.module];
        for function in &mut core.functions {
            for clause in &mut function.clauses {
                if let Some(body) = &mut clause.body.core_expr {
                    *body = super::super::scalar_replacement::scalar_replace_fixed_aggregates(
                        body, layouts,
                    );
                }
            }
        }
    }
    Ok(())
}
