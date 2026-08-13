pub(super) use super::core_expr_lowering::core_expr_from_syntax;
pub(super) use super::test_support::*;
pub(super) use super::*;
pub(super) use crate::terlan_hir::resolve_syntax_module_output;
pub(super) use crate::terlan_syntax::{parse_module_as_syntax_output, SyntaxPatternFieldOutput};

#[cfg(test)]
#[path = "core_lowering_test/interface_and_effects.rs"]
mod interface_and_effects;
#[cfg(test)]
#[path = "core_lowering_test/pattern_coverage.rs"]
mod pattern_coverage;
