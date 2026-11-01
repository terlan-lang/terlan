//! Coverage uses resolved constructor identities, never import alias spellings.

use super::constructors::explicit_constructor_coverage_pattern;
use super::{ExprInferContext, SyntaxPatternKind, SyntaxPatternOutput};

/// Resolves imported constructor heads in a private copy used by coverage.
pub(in crate::compiler::typeck) fn resolve_coverage_pattern(
    pattern: &mut SyntaxPatternOutput,
    ctx: &ExprInferContext<'_>,
) {
    for child in &mut pattern.children {
        resolve_coverage_pattern(child, ctx);
    }
    for field in &mut pattern.fields {
        resolve_coverage_pattern(&mut field.value, ctx);
    }
    if pattern.kind == SyntaxPatternKind::Constructor {
        if let Some(name) = pattern.text.as_ref() {
            if let Some(structural) =
                explicit_constructor_coverage_pattern(name, &pattern.children, ctx)
            {
                *pattern = structural;
                return;
            }
            if !ctx.constructors.contains_key(name) {
                if let Some(imported) = ctx.constructor_aliases.get(name) {
                    if let Some(module) = imported.module.as_ref() {
                        pattern.text = Some(format!("{module}.{}", imported.name));
                    }
                }
            }
        }
    }
}
