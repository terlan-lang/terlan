use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::terlan_hir::module_path_to_native_boundary_module;
use crate::terlan_syntax::syntax_output::SyntaxAnnotationValueOutput;
use crate::terlan_syntax::{
    parse_module_as_syntax_output, SyntaxDeclarationPayload, SyntaxModuleOutput,
};
use serde_json::json;

use crate::validation::native_policy::NativePolicy;

/// Native metadata emitted for a Terlan source module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeMetadata {
    pub(crate) source_module: String,
    pub(crate) native_module: String,
    pub(crate) scheduler: String,
    pub(crate) native_policy: NativePolicy,
    pub(crate) functions: Vec<NativeFunctionSignature>,
}

/// Native function export signature discovered from a native declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeFunctionSignature {
    pub(crate) name: String,
    pub(crate) arity: usize,
    pub(crate) operation: Option<String>,
}

impl NativeMetadata {
    /// Serializes native metadata to stable JSON text.
    ///
    /// Inputs:
    /// - `self`: extracted native metadata.
    ///
    /// Output:
    /// - Pretty JSON text ending in a trailing newline.
    ///
    /// Transformation:
    /// - Serializes string fields through `serde_json` and renders function
    ///   signatures as name/arity objects with optional compiler-native
    ///   operation identifiers.
    pub(crate) fn to_json(&self) -> String {
        let functions = self
            .functions
            .iter()
            .map(|function| {
                let mut value = json!({
                    "name": function.name,
                    "arity": function.arity,
                });
                if let Some(operation) = &function.operation {
                    value["operation"] = json!(operation);
                }
                value
            })
            .collect::<Vec<_>>();
        let metadata = json!({
            "source_module": self.source_module,
            "module": self.native_module,
            "scheduler": self.scheduler,
            "native_policy": self.native_policy.as_str(),
            "functions": functions,
        });
        let mut rendered =
            serde_json::to_string_pretty(&metadata).expect("native metadata JSON should serialize");
        rendered.push('\n');
        rendered
    }
}

/// Emits NativeBoundary metadata and Rust adapter skeletons.
///
/// Inputs:
/// - `source`: Terlan source text containing `@compiler.native` declarations.
/// - `out_dir`: destination directory for generated artifacts.
/// - `policy`: selected native policy to record in metadata.
/// - `incremental`: when true, unchanged outputs are left untouched.
///
/// Output:
/// - `Ok(())` when metadata and Rust stub are written.
/// - `Err(String)` for missing metadata fields, invalid generated Rust, or
///   filesystem failures.
///
/// Transformation:
/// - Extracts native metadata from source, renders JSON plus a NativeBoundary Rust
///   skeleton, validates the Rust stub ownership contract, and writes outputs.
pub(crate) fn emit_native_artifacts(
    source: &str,
    out_dir: &Path,
    policy: NativePolicy,
    incremental: bool,
) -> Result<(), String> {
    let metadata = extract_native_metadata(source, policy)?;
    if let Err(err) = fs::create_dir_all(out_dir) {
        return Err(format!("cannot create output directory: {}", err));
    }

    let metadata_target = out_dir.join(format!("{}.native_boundary.json", metadata.source_module));
    crate::support::write_if_changed_or_forced(
        &metadata_target,
        metadata.to_json().as_bytes(),
        incremental,
    )
    .map_err(|err| format!("failed to write native metadata: {}", err))?;

    let rust_stub_target = out_dir.join(format!("{}.native_boundary.rs", metadata.native_module));
    let rust_stub = emit_native_boundary_rust_stub(&metadata);
    validate_native_boundary_rust_stub(&rust_stub).map_err(|err| {
        format!(
            "generated NativeBoundary Rust stub violates ownership contract: {}",
            err
        )
    })?;
    crate::support::write_if_changed_or_forced(
        &rust_stub_target,
        rust_stub.as_bytes(),
        incremental,
    )
    .map_err(|err| format!("failed to write native rust stub: {}", err))?;

    Ok(())
}

/// Validates generated Rust stub text against the NativeBoundary contract.
///
/// Inputs:
/// - `stub`: generated Rust source text.
///
/// Output:
/// - `Ok(())` when forbidden unsafe patterns are absent.
/// - `Err(String)` naming the first forbidden pattern found.
///
/// Transformation:
/// - Performs a conservative textual scan before the stub is written.
pub(crate) fn validate_native_boundary_rust_stub(stub: &str) -> Result<(), String> {
    const FORBIDDEN_PATTERNS: [&str; 9] = [
        "unsafe fn",
        "unsafe extern",
        "unsafe impl",
        "unsafe trait",
        "unsafe {",
        " *mut ",
        " *const ",
        "std::ptr::",
        "std::mem::transmute",
    ];

    for pattern in FORBIDDEN_PATTERNS {
        if stub.contains(pattern) {
            return Err(format!("forbidden pattern `{}`", pattern));
        }
    }
    Ok(())
}

/// Extracts NativeBoundary metadata from Terlan source text.
///
/// Inputs:
/// - `source`: Terlan source text.
/// - `requested_policy`: native policy selected by the command.
///
/// Output:
/// - `Ok(NativeMetadata)` when module and compiler-native function signatures
///   are available.
/// - `Err(String)` when a required metadata field is absent.
///
/// Transformation:
/// - Derives NativeBoundary metadata from `@compiler.native {operation}` annotated
///   declarations. Pure policy is normalized to native-boundary optional whenever
///   compiler-native declarations are present.
pub(crate) fn extract_native_metadata(
    source: &str,
    requested_policy: NativePolicy,
) -> Result<NativeMetadata, String> {
    let syntax = parse_module_as_syntax_output(source)
        .map_err(|error| format!("native metadata source could not be parsed: {error:?}"))?;
    let source_module = syntax.module_name.clone();
    let compiler_native_functions =
        dedupe_native_function_signatures(extract_compiler_native_functions(&syntax));
    if compiler_native_functions.is_empty() {
        return Err("native metadata source is missing @compiler.native declarations".to_string());
    }
    let native_module = module_path_to_native_boundary_module(&source_module);
    let scheduler = "normal".to_string();
    let native_policy = if requested_policy == NativePolicy::Pure {
        NativePolicy::NativeBoundaryOptional
    } else {
        requested_policy
    };

    Ok(NativeMetadata {
        source_module,
        native_module,
        scheduler,
        native_policy,
        functions: compiler_native_functions,
    })
}

/// Reads binding declarations from the canonical parser, including private
/// functions used by source-owned public APIs.
fn extract_compiler_native_functions(module: &SyntaxModuleOutput) -> Vec<NativeFunctionSignature> {
    module
        .declarations
        .iter()
        .filter_map(|declaration| {
            let operation = declaration.annotations.iter().find_map(|annotation| {
                if annotation.path != ["compiler", "native"] {
                    return None;
                }
                match annotation.values.first()? {
                    SyntaxAnnotationValueOutput::Name { segments } if !segments.is_empty() => {
                        Some(segments.join("."))
                    }
                    _ => None,
                }
            })?;
            let (name, arity) = match &declaration.payload {
                SyntaxDeclarationPayload::Function { name, params, .. } => (name, params.len()),
                SyntaxDeclarationPayload::Method { name, params, .. } => (name, params.len() + 1),
                _ => return None,
            };
            Some(NativeFunctionSignature {
                name: name.clone(),
                arity,
                operation: Some(operation),
            })
        })
        .collect()
}

/// Removes duplicate native backend signatures while preserving source order.
///
/// Inputs:
/// - `functions`: native declarations extracted from source annotations.
///
/// Output:
/// - Function signatures with duplicate `(name, arity, operation)` rows
///   removed.
///
/// Transformation:
/// - Keeps the first occurrence of each backend signature so source-level
///   overloads may share a native operation without generating duplicate
///   metadata rows or duplicate Rust match arms.
fn dedupe_native_function_signatures(
    functions: Vec<NativeFunctionSignature>,
) -> Vec<NativeFunctionSignature> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    for function in functions {
        let key = (
            function.name.clone(),
            function.arity,
            function.operation.clone(),
        );
        if seen.insert(key) {
            out.push(function);
        }
    }

    out
}

/// Renders a Rust NativeBoundary skeleton.
///
/// Inputs:
/// - `metadata`: extracted native metadata.
///
/// Output:
/// - Rust source text for a safe actor-bridge skeleton.
///
/// Transformation:
/// - Emits constants for metadata, opaque handle types, typed replies, and a
///   worker object that owns its channel and thread join handle without unsafe
///   code.
fn emit_native_boundary_rust_stub(metadata: &NativeMetadata) -> String {
    let mut out = String::new();
    out.push_str("#![forbid(unsafe_code)]\n");
    out.push_str("// AUTO-GENERATED NativeBoundary skeleton.\n");
    out.push_str(
        "// Implement concrete native exports only after preserving this bridge contract.\n\n",
    );
    out.push_str("use std::collections::HashMap;\n");
    out.push_str("use std::sync::mpsc::{self, Receiver, Sender};\n");
    out.push_str("use std::thread::{self, JoinHandle};\n\n");
    out.push_str(&format!(
        "pub const SOURCE_MODULE: &str = \"{}\";\n",
        metadata.source_module
    ));
    out.push_str(&format!(
        "pub const NATIVE_MODULE: &str = \"{}\";\n",
        metadata.native_module
    ));
    out.push_str(&format!(
        "pub const SCHEDULER: &str = \"{}\";\n",
        metadata.scheduler
    ));
    out.push_str("\npub const FUNCTIONS: &[(&str, usize)] = &[\n");
    for function in &metadata.functions {
        out.push_str(&format!(
            "    (\"{}\", {}),\n",
            function.name, function.arity
        ));
    }
    out.push_str("];\n\n");
    out.push_str("pub const OPERATIONS: &[(&str, &str, usize)] = &[\n");
    for function in &metadata.functions {
        let operation = function.operation.as_deref().unwrap_or(&function.name);
        out.push_str(&format!(
            "    (\"{}\", \"{}\", {}),\n",
            function.name, operation, function.arity
        ));
    }
    out.push_str("];\n\n");
    out.push_str("pub const DEFAULT_CREDIT_WINDOW: usize = 32;\n\n");
    out.push_str(
        "// Rust owns native resources. VM/Terlan terms should hold only opaque handles.\n",
    );
    out.push_str("#[derive(Clone, Debug, PartialEq, Eq)]\n");
    out.push_str("pub struct NativeBoundaryHandle {\n");
    out.push_str("    pub id: u64,\n");
    out.push_str("    pub generation: u64,\n");
    out.push_str("    pub type_name: &'static str,\n");
    out.push_str("}\n\n");
    out.push_str("#[derive(Clone, Debug, PartialEq, Eq)]\n");
    out.push_str("pub struct NativeBoundaryError {\n");
    out.push_str("    pub code: &'static str,\n");
    out.push_str("    pub message: String,\n");
    out.push_str("    pub offset: usize,\n");
    out.push_str("}\n\n");
    out.push_str("#[derive(Clone, Debug, PartialEq)]\n");
    out.push_str("pub enum NativeBoundaryValue {\n");
    out.push_str("    Unit,\n");
    out.push_str("    Text(String),\n");
    out.push_str("    Int(i64),\n");
    out.push_str("    Float(f64),\n");
    out.push_str("    Bool(bool),\n");
    out.push_str("    Handle(NativeBoundaryHandle),\n");
    out.push_str("    OptionalText(Option<String>),\n");
    out.push_str("    OptionalHandle(Option<NativeBoundaryHandle>),\n");
    out.push_str("}\n\n");
    out.push_str("#[derive(Clone, Debug, PartialEq)]\n");
    out.push_str("pub struct NativeBoundaryReply {\n");
    out.push_str("    pub request_id: u64,\n");
    out.push_str("    pub result: Result<NativeBoundaryValue, NativeBoundaryError>,\n");
    out.push_str("    pub credits: usize,\n");
    out.push_str("}\n\n");
    out.push_str("pub struct NativeBoundaryWorker {\n");
    out.push_str("    tx: Sender<NativeBoundaryCommand>,\n");
    out.push_str("    join: Option<JoinHandle<()>>,\n");
    out.push_str("    credit_window: usize,\n");
    out.push_str("}\n\n");
    out.push_str("enum NativeBoundaryCommand {\n");
    out.push_str(
        "    Register { request_id: u64, type_name: &'static str, reply: Sender<NativeBoundaryReply> },\n",
    );
    out.push_str(
        "    Call { request_id: u64, operation: &'static str, args: Vec<NativeBoundaryValue>, reply: Sender<NativeBoundaryReply> },\n",
    );
    out.push_str(
        "    Dispose { request_id: u64, handle: NativeBoundaryHandle, reply: Sender<NativeBoundaryReply> },\n",
    );
    out.push_str("    Stop,\n");
    out.push_str("}\n\n");
    out.push_str("impl NativeBoundaryWorker {\n");
    out.push_str("    pub fn start(credit_window: usize) -> Self {\n");
    out.push_str("        let credit_window = credit_window.max(1);\n");
    out.push_str("        let (tx, rx) = mpsc::channel();\n");
    out.push_str("        let join = thread::spawn(move || worker_loop(rx, credit_window));\n");
    out.push_str("        Self { tx, join: Some(join), credit_window }\n");
    out.push_str("    }\n\n");
    out.push_str("    pub fn credit_window(&self) -> usize {\n");
    out.push_str("        self.credit_window\n");
    out.push_str("    }\n\n");
    out.push_str("    pub fn register_resource(&self, request_id: u64, type_name: &'static str) -> NativeBoundaryReply {\n");
    out.push_str("        let (reply, rx) = mpsc::channel();\n");
    out.push_str("        self.send_and_recv(NativeBoundaryCommand::Register { request_id, type_name, reply }, request_id, rx)\n");
    out.push_str("    }\n\n");
    out.push_str("    pub fn call(&self, request_id: u64, operation: &'static str, args: Vec<NativeBoundaryValue>) -> NativeBoundaryReply {\n");
    out.push_str("        let (reply, rx) = mpsc::channel();\n");
    out.push_str("        self.send_and_recv(NativeBoundaryCommand::Call { request_id, operation, args, reply }, request_id, rx)\n");
    out.push_str("    }\n\n");
    out.push_str(
        "    pub fn dispose(&self, request_id: u64, handle: NativeBoundaryHandle) -> NativeBoundaryReply {\n",
    );
    out.push_str("        let (reply, rx) = mpsc::channel();\n");
    out.push_str(
        "        self.send_and_recv(NativeBoundaryCommand::Dispose { request_id, handle, reply }, request_id, rx)\n",
    );
    out.push_str("    }\n\n");
    out.push_str("    pub fn request_stop(&self) {\n");
    out.push_str("        let _ = self.tx.send(NativeBoundaryCommand::Stop);\n");
    out.push_str("    }\n\n");
    out.push_str("    pub fn stop(mut self) {\n");
    out.push_str("        self.request_stop();\n");
    out.push_str("        if let Some(join) = self.join.take() {\n");
    out.push_str("            let _ = join.join();\n");
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push('\n');
    out.push_str("    fn send_and_recv(&self, command: NativeBoundaryCommand, request_id: u64, rx: Receiver<NativeBoundaryReply>) -> NativeBoundaryReply {\n");
    out.push_str("        if self.tx.send(command).is_err() {\n");
    out.push_str("            return native_error_reply(request_id, \"native_worker_stopped\", \"native worker is not accepting requests\", 0);\n");
    out.push_str("        }\n");
    out.push_str("        rx.recv().unwrap_or_else(|_| native_error_reply(request_id, \"native_worker_stopped\", \"native worker stopped before replying\", 0))\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");
    out.push_str("impl Drop for NativeBoundaryWorker {\n");
    out.push_str("    fn drop(&mut self) {\n");
    out.push_str("        let _ = self.tx.send(NativeBoundaryCommand::Stop);\n");
    out.push_str("        if let Some(join) = self.join.take() {\n");
    out.push_str("            let _ = join.join();\n");
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");
    out.push_str("#[derive(Clone, Debug, PartialEq, Eq)]\n");
    out.push_str("struct ResourceState {\n");
    out.push_str("    generation: u64,\n");
    out.push_str("    type_name: &'static str,\n");
    out.push_str("}\n\n");
    out.push_str("fn worker_loop(rx: Receiver<NativeBoundaryCommand>, credit_window: usize) {\n");
    out.push_str("    let mut next_id = 1_u64;\n");
    out.push_str("    let mut resources = HashMap::<u64, ResourceState>::new();\n");
    out.push_str("    while let Ok(command) = rx.recv() {\n");
    out.push_str("        match command {\n");
    out.push_str(
        "            NativeBoundaryCommand::Register { request_id, type_name, reply } => {\n",
    );
    out.push_str("                let id = next_id;\n");
    out.push_str("                next_id += 1;\n");
    out.push_str(
        "                let handle = NativeBoundaryHandle { id, generation: 1, type_name };\n",
    );
    out.push_str("                resources.insert(id, ResourceState { generation: handle.generation, type_name });\n");
    out.push_str("                let _ = reply.send(NativeBoundaryReply { request_id, result: Ok(NativeBoundaryValue::Handle(handle)), credits: credit_window });\n");
    out.push_str("            }\n");
    out.push_str(
        "            NativeBoundaryCommand::Call { request_id, operation, args, reply } => {\n",
    );
    out.push_str("                let result = match validate_args(&resources, &args) {\n");
    out.push_str("                    Ok(()) => match operation {\n");
    for function in &metadata.functions {
        let operation = function.operation.as_deref().unwrap_or(&function.name);
        out.push_str(&format!(
            "                        \"{}\" => native_unimplemented_operation(operation),\n",
            escape_rust_string(operation)
        ));
    }
    out.push_str("                        _ => native_unknown_operation(operation),\n");
    out.push_str("                    },\n");
    out.push_str("                    Err(err) => Err(err),\n");
    out.push_str("                };\n");
    out.push_str("                let _ = reply.send(NativeBoundaryReply { request_id, result, credits: credit_window });\n");
    out.push_str("            }\n");
    out.push_str("            NativeBoundaryCommand::Dispose { request_id, handle, reply } => {\n");
    out.push_str("                let result = match validate_handle(&resources, &handle) {\n");
    out.push_str("                    Ok(()) => {\n");
    out.push_str("                        resources.remove(&handle.id);\n");
    out.push_str("                        Ok(NativeBoundaryValue::Unit)\n");
    out.push_str("                    }\n");
    out.push_str("                    Err(err) => Err(err),\n");
    out.push_str("                };\n");
    out.push_str("                let _ = reply.send(NativeBoundaryReply { request_id, result, credits: credit_window });\n");
    out.push_str("            }\n");
    out.push_str("            NativeBoundaryCommand::Stop => break,\n");
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");
    out.push_str("fn native_unimplemented_operation(operation: &'static str) -> Result<NativeBoundaryValue, NativeBoundaryError> {\n");
    out.push_str("    Err(NativeBoundaryError { code: \"native_operation_unimplemented\", message: format!(\"native operation {} is declared but not implemented\", operation), offset: 0 })\n");
    out.push_str("}\n\n");
    out.push_str("fn native_unknown_operation(operation: &'static str) -> Result<NativeBoundaryValue, NativeBoundaryError> {\n");
    out.push_str("    Err(NativeBoundaryError { code: \"native_operation_unknown\", message: format!(\"native operation {} is not declared in this adapter\", operation), offset: 0 })\n");
    out.push_str("}\n\n");
    out.push_str("fn validate_args(resources: &HashMap<u64, ResourceState>, args: &[NativeBoundaryValue]) -> Result<(), NativeBoundaryError> {\n");
    out.push_str("    for arg in args {\n");
    out.push_str("        validate_value_arg(resources, arg)?;\n");
    out.push_str("    }\n");
    out.push_str("    Ok(())\n");
    out.push_str("}\n\n");
    out.push_str("fn validate_value_arg(resources: &HashMap<u64, ResourceState>, arg: &NativeBoundaryValue) -> Result<(), NativeBoundaryError> {\n");
    out.push_str("    match arg {\n");
    out.push_str(
        "        NativeBoundaryValue::Handle(handle) => validate_handle(resources, handle),\n",
    );
    out.push_str("        NativeBoundaryValue::OptionalHandle(Some(handle)) => validate_handle(resources, handle),\n");
    out.push_str("        _ => Ok(()),\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");
    out.push_str("fn validate_handle(resources: &HashMap<u64, ResourceState>, handle: &NativeBoundaryHandle) -> Result<(), NativeBoundaryError> {\n");
    out.push_str("    match resources.get(&handle.id) {\n");
    out.push_str("        Some(resource) if resource.generation == handle.generation && resource.type_name == handle.type_name => Ok(()),\n");
    out.push_str("        _ => Err(NativeBoundaryError { code: \"stale_native_handle\", message: format!(\"native handle {} generation {} is not live\", handle.id, handle.generation), offset: 0 }),\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");
    out.push_str("fn native_error_reply(request_id: u64, code: &'static str, message: &str, credits: usize) -> NativeBoundaryReply {\n");
    out.push_str("    NativeBoundaryReply { request_id, result: Err(NativeBoundaryError { code, message: message.to_string(), offset: 0 }), credits }\n");
    out.push_str("}\n");
    out
}

/// Escapes text for a generated Rust string literal body.
///
/// Inputs:
/// - `input`: raw string content.
///
/// Output:
/// - Text safe to place between double quotes in generated Rust source.
///
/// Transformation:
/// - Escapes Rust quote, slash, and common control characters used by generated
///   NativeBoundary operation names.
fn escape_rust_string(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[cfg(test)]
#[path = "artifacts_test.rs"]
#[cfg(test)]
mod artifacts_test;
