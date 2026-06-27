use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::{HashMap, HashSet};

    use crate::terlan_hir::{
        load_interfaces_from_dir, load_interfaces_from_file_set,
        resolve_syntax_module_output_with_interfaces, ModuleInterface,
    };
    use crate::terlan_syntax::{parse_module_as_syntax_output, SyntaxModuleOutput};
    use crate::terlan_typeck::{
        CoreCaseClause, CoreCheckedPreservationEvidence, CoreCheckedPreservationEvidenceKind,
        CoreFunction, CoreFunctionClause, CoreIfClause, CoreModuleMetadata, CoreParam,
        CoreProofReadiness, CoreSourceIdentity, CoreSubstitutionFreshnessEvidence, CORE_IR_SCHEMA,
    };

    mod a0_progression_test;
    mod direct_core_shape_test;
    mod std_bridge_test;
    mod target_family_test;

    /// Lowers source text to a typed Core module through the formal syntax-output
    /// path.
    ///
    /// Inputs:
    /// - `source`: Terlan source module text.
    /// - `path`: synthetic source path used for interface lookup identity.
    ///
    /// Output:
    /// - Lowered `CoreModule` containing expression and pattern summaries.
    ///
    /// Transformation:
    /// - Parses source as syntax output, resolves it with file-set interfaces,
    ///   and lowers the result to backend-agnostic CoreIR.
    fn lower(source: &str, path: &str) -> CoreModule {
        let syntax: SyntaxModuleOutput =
            parse_module_as_syntax_output(source).expect("parse syntax output");
        let mut interfaces = load_interfaces_from_file_set(path);
        if interfaces.is_empty() {
            let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            load_interfaces_from_dir(&workspace_root.join("std/summaries"), &mut interfaces);
        }
        let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
        crate::terlan_typeck::lower_syntax_module_output_to_core(&syntax, &resolved)
    }
}
