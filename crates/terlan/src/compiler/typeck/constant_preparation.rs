//! Resolves constants before preparing checked syntax.

use super::*;

pub(super) fn prepare_syntax_constants_with_interfaces(
    module: &SyntaxModuleOutput,
    interfaces: &HashMap<String, ModuleInterface>,
) -> (SyntaxModuleOutput, Vec<Diagnostic>) {
    if !crate::value_lifecycle::module_requires_value_lifecycle_pass(module, interfaces) {
        return (module.clone(), Vec::new());
    }
    let mut prepared = module.clone();
    let report = crate::value_lifecycle::evaluate_and_substitute_module_constants_with_interfaces(
        &mut prepared,
        interfaces,
    );
    let diagnostics = report
        .diagnostics
        .into_iter()
        .map(|diagnostic| Diagnostic {
            span: diagnostic.span.into(),
            message: format!("{}: {}", diagnostic.code, diagnostic.message),
            severity: DiagSeverity::Error,
        })
        .collect();
    (prepared, diagnostics)
}
