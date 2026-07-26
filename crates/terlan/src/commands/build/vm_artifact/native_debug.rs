//! Compiler-owned source identities embedded in native TVM images.

use crate::compiler::native_ir::NativeModule;
use crate::runtime::native_image::debug::{encode_tvm_native_debug, TvmNativeDebugRecord};
use crate::terlan_syntax::{SyntaxDeclarationPayload, SyntaxModuleOutput};
use crate::terlan_typeck::CoreModule;

/// Compiler artifacts required to derive source identities for one module.
pub(crate) struct NativeDebugInput<'a> {
    /// Stable source path embedded into every function record.
    pub(crate) source_path: &'a str,
    /// Exact UTF-8 source used to validate declaration spans.
    pub(crate) source_text: &'a str,
    /// Checked CoreIR module that produced native functions.
    pub(crate) core: &'a CoreModule,
    /// Parsed declarations carrying compiler-owned source spans.
    pub(crate) syntax: &'a SyntaxModuleOutput,
}

/// Derives and canonically encodes native source records for compiled modules.
pub(crate) fn encode_native_debug(
    inputs: &[NativeDebugInput<'_>],
    natives: &[NativeModule],
) -> Result<Vec<u8>, String> {
    let mut records = Vec::new();
    for native in natives {
        for function in &native.functions {
            let input = inputs
                .iter()
                .find(|input| input.core.module == function.source_module)
                .ok_or_else(|| {
                    format!(
                        "error[tvm.debug.module]: missing source for `{}` (native `{}`)",
                        function.source_module, native.name
                    )
                })?;
            let (mut span_start, span_end) =
                function_source_span(
                    input.syntax,
                    &function.source_function,
                    function.source_arity,
                )
                .ok_or_else(|| {
                        format!(
                            "error[tvm.debug.function]: `{}` has no declaration for `{}/{}` (native `{}.{}/{}`)",
                            function.source_module,
                            function.source_function,
                            function.source_arity,
                            native.name,
                            function.name,
                            function.arity
                        )
                    })?;
            if span_start >= span_end
                || span_end > input.source_text.len()
                || !input.source_text.is_char_boundary(span_start)
                || !input.source_text.is_char_boundary(span_end)
            {
                return Err(format!(
                    "error[tvm.debug.span]: `{}.{}/{}` has invalid source span {span_start}..{span_end}",
                    native.name, function.name, function.arity
                ));
            }
            let declaration = &input.source_text[span_start..span_end];
            span_start += declaration.len() - declaration.trim_start().len();
            records.push(TvmNativeDebugRecord {
                source_file: input.source_path.to_string(),
                module: native.name.clone(),
                function: function.name.clone(),
                arity: function.arity,
                span_start,
                span_end,
                core_schema: input.core.schema.clone(),
                proof_readiness: format!("{:?}", input.core.metadata.proof_readiness),
            });
        }
    }
    records.sort_by(|left, right| {
        left.module
            .cmp(&right.module)
            .then_with(|| left.function.cmp(&right.function))
            .then_with(|| left.arity.cmp(&right.arity))
    });
    encode_tvm_native_debug(&records)
}

/// Finds the combined declaration span for one exact function identity.
fn function_source_span(
    syntax: &SyntaxModuleOutput,
    function_name: &str,
    arity: usize,
) -> Option<(usize, usize)> {
    syntax
        .declarations
        .iter()
        .filter(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Function { name, params, .. } => {
                name == function_name && params.len() == arity
            }
            SyntaxDeclarationPayload::Method { name, params, .. } => {
                name == function_name && params.len() + 1 == arity
            }
            _ => false,
        })
        .map(|declaration| (declaration.span.start, declaration.span.end))
        .reduce(|left, right| (left.0.min(right.0), left.1.max(right.1)))
}
