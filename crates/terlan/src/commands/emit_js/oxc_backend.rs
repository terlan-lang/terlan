#[cfg(test)]
use std::path::Path;

use crate::terlan_typeck::CoreModule;

use super::{core_lowering, direct_ast};

pub(crate) use direct_ast::emit_core_module_with_direct_oxc_ast;

#[cfg(test)]
pub(crate) use direct_ast::emit_minimal_direct_oxc_ast_module;

/// Emits a CoreIR module as JavaScript through the current Oxc backend path.
///
/// Inputs:
/// - `module`: backend-independent CoreIR module from the formal pipeline.
///
/// Output:
/// - `Ok(String)` containing JavaScript source printed by Oxc codegen.
/// - `Err(String)` when the fallback bootstrap JS cannot be parsed as a module
///   by Oxc.
///
/// Transformation:
/// - Attempts direct CoreIR-to-Oxc-AST construction for the supported subset.
///   Unsupported modules fall back to bootstrap CoreIR-to-JS text lowering and
///   are then parsed/reprinted through Oxc codegen.
pub(crate) fn emit_core_module_with_oxc_codegen(module: &CoreModule) -> Result<String, String> {
    if let Some(js) = direct_ast::emit_core_module_with_direct_oxc_ast(module) {
        return Ok(js);
    }
    emit_js_with_oxc_codegen(&core_lowering::emit_core_module_to_js(module))
}

/// Parses and reprints JavaScript through Oxc.
///
/// Inputs:
/// - `source`: JavaScript module source produced by the current CoreIR lowering
///   bootstrap.
///
/// Output:
/// - `Ok(String)` containing JavaScript source printed by Oxc codegen.
/// - `Err(String)` describing Oxc parser diagnostics when the source is not a
///   valid ECMAScript module.
///
/// Transformation:
/// - Parses source with Oxc as an ECMAScript module, rejects parser
///   diagnostics, then prints the resulting Oxc AST through `oxc_codegen`.
pub(crate) fn emit_js_with_oxc_codegen(source: &str) -> Result<String, String> {
    let allocator = oxc_allocator::Allocator::default();
    let source_type = oxc_span::SourceType::mjs();
    let parsed = oxc_parser::Parser::new(&allocator, source, source_type).parse();
    if !parsed.errors.is_empty() {
        return Err(format!("{:?}", parsed.errors));
    }
    Ok(oxc_codegen::Codegen::new().build(&parsed.program).code)
}

/// Validates JavaScript source as an ECMAScript module through Oxc.
///
/// Inputs:
/// - `source`: JavaScript module source produced by a release or probe
///   emitter.
///
/// Output:
/// - `Ok(())` when Oxc parses and codegens the module successfully.
/// - `Err(String)` containing parser diagnostics when Oxc rejects the source.
///
/// Transformation:
/// - Reuses the parser/codegen validation path without returning the reprinted
///   JavaScript, giving build commands a mandatory JS validation hook that is
///   independent of any external runtime.
pub(crate) fn validate_js_module_with_oxc(source: &str) -> Result<(), String> {
    emit_js_with_oxc_codegen(source).map(|_| ())
}

/// Validates JavaScript source as an ECMAScript module through Oxc.
///
/// Inputs:
/// - `path`: generated JavaScript artifact path used in diagnostics.
/// - `source`: JavaScript source text written by `terlc emit-js`.
///
/// Output:
/// - Panics when Oxc reports parser diagnostics.
/// - Returns normally when the generated artifact parses as a module.
///
/// Transformation:
/// - Delegates to `emit_js_with_oxc_codegen` and ignores the printed output,
///   using the same parser/codegen path as production JS emission.
#[cfg(test)]
pub(crate) fn assert_oxc_accepts_js_artifact(path: &Path, source: &str) {
    if let Err(message) = validate_js_module_with_oxc(source) {
        panic!("Oxc rejected emitted JS artifact {path:?}: {message}");
    }
}
