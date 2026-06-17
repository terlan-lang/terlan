use std::collections::{BTreeMap, BTreeSet};

use terlan_hir::ModuleInterface;
use terlan_syntax::{
    extract_native_function_signatures, extract_native_module_name, SyntaxConstructorClauseOutput,
    SyntaxConstructorParamOutput, SyntaxDeclarationOutput, SyntaxDeclarationPayload,
    SyntaxExprFieldOutput, SyntaxExprKind, SyntaxExprOutput, SyntaxFunctionClauseOutput,
    SyntaxHtmlAttrOutput, SyntaxHtmlAttrValueOutput, SyntaxHtmlElementOutput, SyntaxHtmlNodeOutput,
    SyntaxImplMethodOutput, SyntaxImportKind, SyntaxModuleOutput, SyntaxParamOutput,
    SyntaxPatternFieldOutput, SyntaxPatternKind, SyntaxPatternOutput, SyntaxSourceKind,
    SyntaxStructFieldOutput, SyntaxTypeOutput,
};
use terlan_typeck::{CoreModule, CORE_IR_SCHEMA};

mod beam_process;
mod core;
mod erl;
mod runtime;
mod syntax;
mod util;

use core::*;
use erl::*;
pub use runtime::emit_html_runtime_to_erlang;
use syntax::{lower_syntax_module_output, lower_syntax_struct_headers_to_hrl};
use util::*;

pub fn try_emit_syntax_module_output_to_erlang(
    module: &SyntaxModuleOutput,
) -> Result<String, String> {
    lower_syntax_module_output(
        module,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .map(|module| module.render())
    .ok_or_else(|| unsupported_direct_syntax_emit_message(module))
}

pub fn try_emit_syntax_module_output_to_erlang_with_interfaces(
    module: &SyntaxModuleOutput,
    interfaces: &BTreeMap<String, ModuleInterface>,
) -> Result<String, String> {
    lower_syntax_module_output(
        module,
        interfaces,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .map(|module| module.render())
    .ok_or_else(|| unsupported_direct_syntax_emit_message(module))
}

pub fn try_emit_syntax_module_output_to_erlang_with_interfaces_file_imports_templates_and_markdown(
    module: &SyntaxModuleOutput,
    interfaces: &BTreeMap<String, ModuleInterface>,
    file_imports: &BTreeMap<String, Vec<u8>>,
    templates: &BTreeMap<String, terlan_html::HtmlTemplate>,
    markdown_imports: &BTreeMap<String, terlan_html::MarkdownDocument>,
) -> Result<String, String> {
    lower_syntax_module_output(
        module,
        interfaces,
        file_imports,
        templates,
        markdown_imports,
    )
    .map(|module| module.render())
    .ok_or_else(|| unsupported_direct_syntax_emit_message(module))
}

/// Emits Erlang through the transitional CoreIR-gated backend entry point.
///
/// Inputs:
/// - `core`: CoreIR module produced by the formal typechecking path.
/// - `module`: syntax output retained as a temporary bridge payload until
///   CoreIR carries full executable expression trees.
/// - `interfaces`: known imported module interfaces.
/// - `file_imports`: loaded raw file-import bytes.
/// - `templates`: loaded HTML template inputs.
/// - `markdown_imports`: loaded Markdown inputs.
///
/// Output:
/// - Erlang source text when CoreIR identity validation and syntax-output
///   bridge lowering both succeed.
/// - `Err(String)` when the CoreIR payload is stale/mismatched or the
///   bridge lowering does not cover the module.
///
/// Transformation:
/// - Validates that the backend is being driven by the current CoreIR schema
///   and by a syntax-output artifact matching the CoreIR source identity, then
///   delegates to the existing syntax-output lowering bridge for the current
///   CoreIR transition slice.
pub fn try_emit_core_module_to_erlang_with_syntax_bridge(
    core: &CoreModule,
    module: &SyntaxModuleOutput,
    interfaces: &BTreeMap<String, ModuleInterface>,
    file_imports: &BTreeMap<String, Vec<u8>>,
    templates: &BTreeMap<String, terlan_html::HtmlTemplate>,
    markdown_imports: &BTreeMap<String, terlan_html::MarkdownDocument>,
) -> Result<String, String> {
    validate_core_module_syntax_bridge(core, module)?;
    try_emit_syntax_module_output_to_erlang_with_interfaces_file_imports_templates_and_markdown(
        module,
        interfaces,
        file_imports,
        templates,
        markdown_imports,
    )
}

/// Validates that a bridge syntax-output payload belongs to CoreIR.
///
/// Inputs:
/// - `core`: CoreIR module selected for backend emission.
/// - `module`: syntax-output payload still needed by the transitional backend.
///
/// Output:
/// - `Ok(())` when schema, module name, source kind, and syntax contract
///   fingerprint match.
/// - `Err(String)` describing the first mismatch.
///
/// Transformation:
/// - Compares CoreIR identity fields against the syntax-output artifact so the
///   backend cannot silently emit from stale source syntax while bypassing the
///   formal CoreIR handoff.
fn validate_core_module_syntax_bridge(
    core: &CoreModule,
    module: &SyntaxModuleOutput,
) -> Result<(), String> {
    if core.schema != CORE_IR_SCHEMA {
        return Err(format!(
            "CoreIR schema mismatch: expected {}, found {}",
            CORE_IR_SCHEMA, core.schema
        ));
    }
    if core.module != module.module_name {
        return Err(format!(
            "CoreIR module mismatch: expected {}, found {}",
            core.module, module.module_name
        ));
    }

    let syntax_source_kind = format!("{:?}", module.source_kind);
    if core.source.source_kind != syntax_source_kind {
        return Err(format!(
            "CoreIR source kind mismatch: expected {}, found {}",
            core.source.source_kind, syntax_source_kind
        ));
    }

    match core.source.syntax_contract_fingerprint.as_deref() {
        Some(fingerprint) if fingerprint == module.syntax_contract.fingerprint => Ok(()),
        Some(fingerprint) => Err(format!(
            "CoreIR syntax contract fingerprint mismatch: expected {}, found {}",
            fingerprint, module.syntax_contract.fingerprint
        )),
        None => Err("CoreIR syntax contract fingerprint is missing".to_string()),
    }
}

pub fn try_emit_syntax_struct_headers_to_hrl(
    module: &SyntaxModuleOutput,
) -> Result<String, String> {
    lower_syntax_struct_headers_to_hrl(module)
        .ok_or_else(|| unsupported_direct_syntax_emit_message(module))
}

fn unsupported_direct_syntax_emit_message(module: &SyntaxModuleOutput) -> String {
    format!(
        "formal Erlang lowering does not yet support module `{}` without the syntax bridge",
        module.module_name
    )
}

#[cfg(test)]
#[path = "emit/core_emit_test.rs"]
mod core_emit_test;
#[cfg(test)]
#[path = "emit_test.rs"]
mod emit_test;
#[cfg(test)]
#[path = "emit/html_emit_test.rs"]
mod html_emit_test;
#[cfg(test)]
#[path = "emit/intrinsic_emit_test.rs"]
mod intrinsic_emit_test;
#[cfg(test)]
#[path = "emit/runtime_emit_test.rs"]
mod runtime_emit_test;
#[cfg(test)]
#[path = "emit/syntax_constructor_emit_test.rs"]
mod syntax_constructor_emit_test;
#[cfg(test)]
#[path = "emit/syntax_emit_test.rs"]
mod syntax_emit_test;
#[cfg(test)]
#[path = "emit/test_support.rs"]
mod test_support;
