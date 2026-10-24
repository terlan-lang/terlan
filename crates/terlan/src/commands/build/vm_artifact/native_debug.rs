//! Compiler-owned source identities embedded in native TVM images.

use std::collections::BTreeMap;

use crate::compiler::native_ir::coverage_callable_id;
use crate::compiler::native_ir::NativeModule;
use crate::runtime::native_image::debug::{
    encode_tvm_native_debug, tvm_debug_source_sha256, TvmNativeDebugContinuationRecord,
    TvmNativeDebugRecord,
};
use crate::terlan_syntax::{SyntaxDeclarationPayload, SyntaxModuleOutput};
use crate::terlan_typeck::CoreModule;

/// Compact compiler-owned source identities for one module.
pub(crate) struct NativeDebugInput {
    /// Stable source path embedded into every function record.
    source_path: String,
    /// Exact UTF-8 source used to validate declaration spans.
    source_text: String,
    module: String,
    core_schema: String,
    proof_readiness: String,
    function_spans: BTreeMap<(String, usize), (usize, usize)>,
}

impl NativeDebugInput {
    /// Captures debug metadata without retaining syntax or CoreIR ownership.
    pub(crate) fn capture(
        source_path: &str,
        source_text: &str,
        core: &CoreModule,
        syntax: &SyntaxModuleOutput,
    ) -> Self {
        Self::capture_owned(
            source_path.to_string(),
            source_text.to_string(),
            core,
            syntax,
        )
    }

    /// Captures debug metadata while taking ownership of source identity data.
    pub(crate) fn capture_owned(
        source_path: String,
        source_text: String,
        core: &CoreModule,
        syntax: &SyntaxModuleOutput,
    ) -> Self {
        let function_spans = core
            .functions
            .iter()
            .filter_map(|function| {
                let (name, arity) = function
                    .source
                    .as_ref()
                    .filter(|source| source.module == core.module)
                    .map_or((&function.name, function.arity), |source| {
                        (&source.function, source.arity)
                    });
                function_source_span(core, syntax, name, arity)
                    .map(|span| ((name.clone(), arity), span))
            })
            .collect();
        Self {
            source_path,
            source_text,
            module: core.module.clone(),
            core_schema: core.schema.clone(),
            proof_readiness: format!("{:?}", core.metadata.proof_readiness),
            function_spans,
        }
    }

    /// Replaces the embedded source path without rebuilding debug metadata.
    pub(crate) fn with_source_path(mut self, source_path: String) -> Self {
        self.source_path = source_path;
        self
    }

    /// Returns the exact source bytes represented by this debug projection.
    pub(crate) fn source_text(&self) -> &str {
        &self.source_text
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn constructor_debug_spans_use_source_callable_identity() {
        let workspace = tempfile::tempdir().unwrap();
        let path = workspace.path().join("app/Cell.terl");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let source = "module app.Cell. pub struct Cell { value: Int }. pub constructor Cell { (value: Int): Cell -> Cell { value: value } }.";
        std::fs::write(&path, source).unwrap();
        let compiled = super::super::compile::compile_vm_module(
            path.to_str().unwrap(),
            &crate::CliState::default(),
        )
        .unwrap();
        let debug = compiled.native_debug;
        assert!(debug.function_spans.contains_key(&("Cell".into(), 1)));
    }
}

/// Derives and canonically encodes native source records for compiled modules.
pub(crate) fn encode_native_debug(
    inputs: &[NativeDebugInput],
    natives: &[NativeModule],
) -> Result<Vec<u8>, String> {
    let mut records = Vec::new();
    for native in natives {
        for function in &native.functions {
            let input = inputs
                .iter()
                .find(|input| input.module == function.source_module)
                .ok_or_else(|| {
                    format!(
                        "error[tvm.debug.module]: missing source for `{}` (native `{}`)",
                        function.source_module, native.name
                    )
                })?;
            let (mut span_start, span_end) = input
                .function_spans
                .get(&(function.source_function.clone(), function.source_arity))
                .copied()
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
            let declaration_text = input.source_text[span_start..span_end].trim();
            let source_origin = if function.name != function.source_function
                || function.arity != function.source_arity
            {
                format!(
                    "generated:{}.{}/{}",
                    function.source_module, function.source_function, function.source_arity
                )
            } else if declaration_text.contains("@template") {
                "template".to_string()
            } else {
                "source".to_string()
            };
            let continuation_spans = native
                .continuations
                .iter()
                .filter(|continuation| {
                    continuation.source_module == function.source_module
                        && continuation.source_function == function.source_function
                        && continuation.source_arity == function.source_arity
                })
                .filter_map(|continuation| {
                    continuation
                        .source_span
                        .map(|span| {
                            (
                                continuation.id,
                                span,
                                continuation.capture_names.clone(),
                            )
                        })
                })
                .map(|(id, span, local_names)| {
                    if span.start >= span.end
                        || span.end > input.source_text.len()
                        || !input.source_text.is_char_boundary(span.start)
                        || !input.source_text.is_char_boundary(span.end)
                    {
                        return Err(format!(
                            "error[tvm.debug.continuation_span]: continuation {id} has invalid source span {}..{}",
                            span.start, span.end
                        ));
                    }
                    Ok(TvmNativeDebugContinuationRecord {
                        id,
                        span_start: span.start,
                        span_end: span.end,
                        local_names,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            records.push(TvmNativeDebugRecord {
                callable_id: coverage_callable_id(
                    &function.source_module,
                    &function.source_function,
                    function.source_arity,
                ),
                source_file: input.source_path.clone(),
                module: native.name.clone(),
                function: function.name.clone(),
                arity: function.arity,
                span_start,
                span_end,
                source_sha256: tvm_debug_source_sha256(input.source_text.as_bytes()),
                source_origin,
                continuation_ids: native
                    .continuations
                    .iter()
                    .filter(|continuation| {
                        continuation.source_module == function.source_module
                            && continuation.source_function == function.source_function
                            && continuation.source_arity == function.source_arity
                    })
                    .map(|continuation| continuation.id)
                    .collect(),
                continuation_spans,
                core_schema: input.core_schema.clone(),
                proof_readiness: input.proof_readiness.clone(),
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
    core: &CoreModule,
    syntax: &SyntaxModuleOutput,
    function_name: &str,
    arity: usize,
) -> Option<(usize, usize)> {
    if let Some(span) = core
        .functions
        .iter()
        .filter_map(|function| function.source.as_ref())
        .find(|source| {
            source.module == core.module
                && source.function == function_name
                && source.arity == arity
        })
        .and_then(|source| source.declaration_span)
    {
        return Some((span.start, span.end));
    }
    syntax
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Function { name, params, .. }
                if name == function_name && params.len() == arity =>
            {
                Some((declaration.span.start, declaration.span.end))
            }
            SyntaxDeclarationPayload::Method { name, params, .. }
                if name == function_name && params.len() + 1 == arity =>
            {
                Some((declaration.span.start, declaration.span.end))
            }
            SyntaxDeclarationPayload::Constructor { name, clauses, .. }
                if name == function_name =>
            {
                clauses
                    .iter()
                    .filter(|clause| clause.params.len() == arity)
                    .map(|clause| (clause.span.start, clause.span.end))
                    .reduce(|left, right| (left.0.min(right.0), left.1.max(right.1)))
            }
            _ => None,
        })
        .reduce(|left, right| (left.0.min(right.0), left.1.max(right.1)))
}
