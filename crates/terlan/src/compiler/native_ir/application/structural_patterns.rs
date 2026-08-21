//! Layout-independent structural normalization performed before case lowering.

use std::collections::HashMap;

use crate::terlan_typeck::CoreModule;

/// Scalar-replaces tuple destructuring before case normalization turns it into
/// managed matching control flow.
pub(super) fn scalar_replace(cores: &mut [CoreModule]) -> Result<(), super::super::NativeIrError> {
    let constructor_modules = cores
        .iter()
        .map(|core| (core.module.as_str(), core.constructors.as_slice()))
        .collect::<Vec<_>>();
    let type_modules = cores
        .iter()
        .map(|core| (core.module.as_str(), core.types.as_slice()))
        .collect::<Vec<_>>();
    let layouts = cores
        .iter()
        .map(|core| {
            super::super::constructors::native_constructor_layouts(
                &constructor_modules,
                &core.module,
            )
            .and_then(|mut layouts| {
                super::super::constructors::install_struct_layouts(
                    &type_modules,
                    &core.module,
                    &mut layouts,
                )?;
                super::super::constructors::install_structural_type_layouts(
                    cores.iter().flat_map(|core| {
                        core.functions.iter().flat_map(|function| {
                            function
                                .params
                                .iter()
                                .filter_map(|parameter| parameter.core_ty.as_ref())
                                .chain(function.core_return_type.iter())
                        })
                    }),
                    &mut layouts,
                )?;
                Ok((core.module.clone(), layouts))
            })
        })
        .collect::<Result<HashMap<_, _>, _>>()?;
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
