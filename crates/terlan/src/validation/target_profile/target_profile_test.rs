use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    use crate::terlan_hir::{resolve_syntax_module_output_with_interfaces, ModuleInterface};
    use crate::terlan_syntax::{parse_module_as_syntax_output, SyntaxModuleOutput};
    use crate::terlan_typeck::{
        CoreCaseClause, CoreCheckedPreservationEvidence, CoreCheckedPreservationEvidenceKind,
        CoreExpr, CoreExprSummary, CoreFunction, CoreFunctionClause, CoreIfClause,
        CoreModuleMetadata, CoreParam, CorePattern, CoreProofReadiness, CoreSourceIdentity,
        CoreSubstitutionFreshnessEvidence, CORE_IR_SCHEMA,
    };
    use std::collections::{HashMap, HashSet};

    #[cfg(test)]
    mod a0_progression_test;
    #[cfg(test)]
    mod binary_pattern_test;
    #[cfg(test)]
    mod direct_core_shape_test;
    #[cfg(test)]
    mod std_bridge_test;
    #[cfg(test)]
    mod target_family_test;

    /// Lowers source text to a typed Core module through the formal syntax-output
    /// path.
    ///
    /// Inputs:
    /// - `source`: Terlan source module text.
    /// - `path`: legacy synthetic source path retained by existing fixtures;
    ///   interface loading is deliberately independent of it.
    ///
    /// Output:
    /// - Lowered `CoreModule` containing expression and pattern summaries.
    ///
    /// Transformation:
    /// - Parses source as syntax output, resolves its direct checked-in std
    ///   interfaces, and lowers the result to backend-agnostic CoreIR.
    fn lower(source: &str, _path: &str) -> CoreModule {
        let syntax: SyntaxModuleOutput =
            parse_module_as_syntax_output(source).expect("parse syntax output");
        let interfaces = imported_std_interfaces(&syntax);
        let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
        crate::terlan_typeck::lower_syntax_module_output_to_core(&syntax, &resolved)
    }

    /// Returns the checked-in interfaces explicitly imported by one fixture.
    fn imported_std_interfaces(module: &SyntaxModuleOutput) -> HashMap<String, ModuleInterface> {
        crate::terlan_hir::checked_in_std_interfaces_for_module(module)
    }

    #[test]
    fn target_profile_fixtures_clone_only_direct_std_imports() {
        let module = parse_module_as_syntax_output(
            "module profile_import_scope.\n\nimport std.core.Task.\nimport std.core.Result.{Ok}.\n\npub main(): Int ->\n    1.\n",
        )
        .expect("parse import-scope fixture");

        let interfaces = imported_std_interfaces(&module);
        let mut modules = interfaces.keys().map(String::as_str).collect::<Vec<_>>();
        modules.sort_unstable();

        assert_eq!(modules, vec!["std.core.Result", "std.core.Task"]);
    }
}
