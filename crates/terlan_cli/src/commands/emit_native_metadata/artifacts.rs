use std::fs;
use std::path::Path;

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
    /// - Escapes string fields and renders function signatures as name/arity
    ///   objects with optional compiler-native operation identifiers.
    pub(crate) fn to_json(&self) -> String {
        let functions =
            self.functions
                .iter()
                .map(|function| {
                    let operation = function.operation.as_ref().map(|operation| {
                        format!(", \"operation\": \"{}\"", escape_json(operation))
                    });
                    format!(
                        "\n    {{ \"name\": \"{}\", \"arity\": {}{} }}",
                        escape_json(&function.name),
                        function.arity,
                        operation.as_deref().unwrap_or("")
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
        format!(
            "{{\n  \"source_module\": \"{}\",\n  \"module\": \"{}\",\n  \"scheduler\": \"{}\",\n  \"native_policy\": \"{}\",\n  \"functions\": [{}]\n}}\n",
            escape_json(&self.source_module),
            escape_json(&self.native_module),
            escape_json(&self.scheduler),
            self.native_policy.as_str(),
            functions
        )
    }
}

/// Emits SafeNative metadata and backend stubs.
///
/// Inputs:
/// - `source`: Terlan source text containing `@compiler.native` declarations.
/// - `out_dir`: destination directory for generated artifacts.
/// - `policy`: selected native policy to record in metadata.
/// - `incremental`: when true, unchanged outputs are left untouched.
///
/// Output:
/// - `Ok(())` when metadata, Erlang stub, and Rust stub are written.
/// - `Err(String)` for missing metadata fields, invalid generated Rust, or
///   filesystem failures.
///
/// Transformation:
/// - Extracts native metadata from source, renders JSON plus SafeNative stubs,
///   validates the Rust stub ownership contract, and writes outputs.
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

    let metadata_target = out_dir.join(format!("{}.safe_native.json", metadata.source_module));
    crate::support::write_if_changed_or_forced(
        &metadata_target,
        metadata.to_json().as_bytes(),
        incremental,
    )
    .map_err(|err| format!("failed to write native metadata: {}", err))?;

    let erl_stub_target = out_dir.join(format!("{}.erl", metadata.native_module));
    crate::support::write_if_changed_or_forced(
        &erl_stub_target,
        emit_safe_native_erl_stub(&metadata).as_bytes(),
        incremental,
    )
    .map_err(|err| format!("failed to write native erl stub: {}", err))?;

    let rust_stub_target = out_dir.join(format!("{}.safe_native.rs", metadata.native_module));
    let rust_stub = emit_safe_native_rust_stub(&metadata);
    validate_safe_native_rust_stub(&rust_stub).map_err(|err| {
        format!(
            "generated SafeNative Rust stub violates ownership contract: {}",
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

/// Validates generated Rust stub text against the SafeNative contract.
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
pub(crate) fn validate_safe_native_rust_stub(stub: &str) -> Result<(), String> {
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

/// Extracts SafeNative metadata from Terlan source text.
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
/// - Derives SafeNative metadata from `@compiler.native {operation}` annotated
///   declarations. Pure policy is normalized to safe-native optional whenever
///   compiler-native declarations are present.
pub(crate) fn extract_native_metadata(
    source: &str,
    requested_policy: NativePolicy,
) -> Result<NativeMetadata, String> {
    let source_module = extract_declared_module_name(source)
        .ok_or_else(|| "native metadata source is missing module declaration".to_string())?;
    let compiler_native_functions = extract_compiler_native_functions(source);
    if compiler_native_functions.is_empty() {
        return Err("native metadata source is missing @compiler.native declarations".to_string());
    }
    let native_module = module_path_to_safe_native_module(&source_module);
    let scheduler = "normal".to_string();
    let native_policy = if requested_policy == NativePolicy::Pure {
        NativePolicy::SafeNativeOptional
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

/// Extracts the declared Terlan module name.
///
/// Inputs:
/// - `source`: Terlan source text.
///
/// Output:
/// - `Some(name)` for a non-empty `module name.` declaration.
/// - `None` when no valid module declaration is found.
///
/// Transformation:
/// - Scans line by line and trims the `module` prefix plus trailing period.
pub(crate) fn extract_declared_module_name(source: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let trimmed = line.trim();
        trimmed
            .strip_prefix("module ")
            .and_then(|rest| rest.strip_suffix('.'))
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
    })
}

/// Extracts compiler-native function signatures from annotated declarations.
///
/// Inputs:
/// - `source`: Terlan source text.
///
/// Output:
/// - Function signature names, arities, and operation ids in source order.
///
/// Transformation:
/// - Pairs each `@compiler.native {operation}` annotation with the following
///   public declaration and counts receiver parameters as part of the backend
///   operation arity.
fn extract_compiler_native_functions(source: &str) -> Vec<NativeFunctionSignature> {
    let mut pending_operation: Option<String> = None;
    let mut out = Vec::new();

    for raw_line in source.lines() {
        let trimmed = raw_line.trim();
        if let Some(operation) = parse_compiler_native_operation(trimmed) {
            pending_operation = Some(operation);
            continue;
        }

        let Some(operation) = pending_operation.as_ref() else {
            continue;
        };

        if trimmed.is_empty() || trimmed.starts_with("/**") || trimmed.starts_with('*') {
            continue;
        }

        if let Some(mut signature) = parse_compiler_native_function_signature(trimmed) {
            signature.operation = Some(operation.clone());
            out.push(signature);
        }
        pending_operation = None;
    }

    out
}

/// Parses a compiler-native operation annotation.
///
/// Inputs:
/// - `line`: one trimmed Terlan source line.
///
/// Output:
/// - `Some(operation)` for `@compiler.native {operation}`.
/// - `None` when the line is not a compiler-native annotation.
///
/// Transformation:
/// - Strips the annotation delimiters and trims the operation id.
fn parse_compiler_native_operation(line: &str) -> Option<String> {
    let rest = line.strip_prefix("@compiler.native")?.trim();
    let operation = rest.strip_prefix('{')?.strip_suffix('}')?.trim();
    if operation.is_empty() {
        None
    } else {
        Some(operation.to_string())
    }
}

/// Parses a compiler-native public function or receiver signature.
///
/// Inputs:
/// - `line`: declaration line immediately following a compiler-native
///   annotation.
///
/// Output:
/// - `Some(NativeFunctionSignature)` when the declaration head is recognized.
/// - `None` for malformed or non-public declaration lines.
///
/// Transformation:
/// - Removes the public prefix, detects receiver syntax, extracts the method
///   name, and counts receiver plus top-level argument-list entries.
fn parse_compiler_native_function_signature(line: &str) -> Option<NativeFunctionSignature> {
    let signature = line.trim().strip_prefix("pub ")?.trim();
    if signature.starts_with('(') {
        return parse_compiler_native_receiver_signature(signature);
    }
    parse_compiler_native_plain_signature(signature)
}

/// Parses a compiler-native plain function signature.
///
/// Inputs:
/// - `signature`: public declaration text after the `pub` prefix.
///
/// Output:
/// - Parsed name and arity, or `None` when the text is not a function head.
///
/// Transformation:
/// - Reads the name before the first argument list and counts top-level
///   arguments inside that list.
fn parse_compiler_native_plain_signature(signature: &str) -> Option<NativeFunctionSignature> {
    let open = signature.find('(')?;
    let close = find_matching_paren(signature, open)?;
    let name = parse_native_function_name(&signature[..open])?;
    let args = &signature[open + 1..close];
    Some(NativeFunctionSignature {
        name,
        arity: native_signature_arity(args),
        operation: None,
    })
}

/// Parses a compiler-native receiver method signature.
///
/// Inputs:
/// - `signature`: public declaration text beginning with receiver syntax.
///
/// Output:
/// - Parsed method name and backend arity, or `None` when malformed.
///
/// Transformation:
/// - Treats the receiver as the first backend argument, then parses the method
///   argument list normally.
fn parse_compiler_native_receiver_signature(signature: &str) -> Option<NativeFunctionSignature> {
    let receiver_close = find_matching_paren(signature, 0)?;
    let after_receiver = signature[receiver_close + 1..].trim();
    let method_open = after_receiver.find('(')?;
    let method_close = find_matching_paren(after_receiver, method_open)?;
    let name = parse_native_function_name(&after_receiver[..method_open])?;
    let args = &after_receiver[method_open + 1..method_close];
    Some(NativeFunctionSignature {
        name,
        arity: native_signature_arity(args) + 1,
        operation: None,
    })
}

/// Derives the SafeNative backend module from a Terlan module path.
///
/// Inputs:
/// - `module`: source module path such as `std.data.Json`.
///
/// Output:
/// - Lower-snake SafeNative module name such as `std_data_json_safe_native`.
///
/// Transformation:
/// - Converts each path segment to lower snake case, joins segments with
///   underscores, and appends the SafeNative suffix.
fn module_path_to_safe_native_module(module: &str) -> String {
    let base = module
        .split('.')
        .filter(|segment| !segment.is_empty())
        .map(identifier_to_snake)
        .collect::<Vec<_>>()
        .join("_");
    format!("{base}_safe_native")
}

/// Converts one identifier segment to lower snake case.
///
/// Inputs:
/// - `segment`: module path segment in Terlan casing.
///
/// Output:
/// - Lower-snake representation.
///
/// Transformation:
/// - Inserts underscores before uppercase boundaries where needed and lowers
///   alphabetic characters.
fn identifier_to_snake(segment: &str) -> String {
    let mut out = String::new();
    let mut previous_was_lower_or_digit = false;
    for ch in segment.chars() {
        if ch.is_ascii_uppercase() {
            if previous_was_lower_or_digit && !out.ends_with('_') {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
            previous_was_lower_or_digit = false;
        } else if ch == '-' {
            if !out.ends_with('_') {
                out.push('_');
            }
            previous_was_lower_or_digit = false;
        } else {
            out.push(ch.to_ascii_lowercase());
            previous_was_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        }
    }
    out
}

/// Finds the closing parenthesis for an opening parenthesis.
///
/// Inputs:
/// - `input`: source fragment containing parentheses.
/// - `open_idx`: byte offset of the opening parenthesis.
///
/// Output:
/// - Byte offset of the matching `)`, or `None` if the parentheses are
///   unbalanced.
///
/// Transformation:
/// - Walks characters from `open_idx` while tracking nested depth.
fn find_matching_paren(input: &str, open_idx: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, ch) in input.char_indices().skip(open_idx) {
        match ch {
            '(' => depth += 1,
            ')' if depth == 1 => return Some(offset),
            ')' => {
                if depth > 0 {
                    depth -= 1;
                }
            }
            _ => {}
        }
    }
    None
}

/// Parses the function name before a native argument list.
///
/// Inputs:
/// - `prefix`: signature text before `(`.
///
/// Output:
/// - `Some(name)` for a non-empty function name.
/// - `None` when the prefix contains no name.
///
/// Transformation:
/// - Trims whitespace and removes generic parameter text after `[`.
fn parse_native_function_name(prefix: &str) -> Option<String> {
    let name = prefix
        .trim()
        .split(|ch: char| ch.is_whitespace() || ch == '[')
        .next()
        .unwrap_or("")
        .trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Counts top-level arguments in a native function signature.
///
/// Inputs:
/// - `args`: text between the outer function-call parentheses.
///
/// Output:
/// - Number of top-level comma-separated arguments.
///
/// Transformation:
/// - Tracks nested parentheses, brackets, and braces so commas inside nested
///   types do not increase arity.
fn native_signature_arity(args: &str) -> usize {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return 0;
    }

    let mut paren_depth = 0isize;
    let mut bracket_depth = 0isize;
    let mut brace_depth = 0isize;
    let mut commas = 0usize;

    for ch in args.chars() {
        match ch {
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            ',' if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                commas += 1;
            }
            _ => {}
        }
    }

    commas + 1
}

/// Renders an Erlang stub for SafeNative loading.
///
/// Inputs:
/// - `metadata`: extracted native metadata.
///
/// Output:
/// - Erlang source text for a stub module.
///
/// Transformation:
/// - Emits `load/0`, `-on_load`, metadata helpers, worker transport
///   placeholders, and exported operation placeholders that fail with the
///   stable SafeNative not-loaded error until a concrete worker transport is
///   attached.
fn emit_safe_native_erl_stub(metadata: &NativeMetadata) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "-module({}).\n",
        escape_erlang_quoted_atom(&metadata.native_module)
    ));
    out.push_str("-export([load/0, metadata/0, operations/0]).\n");
    out.push_str("-export([start_worker/1, call_worker/3, dispose_worker/2, stop_worker/1]).\n");
    out.push_str("-on_load(load/0).\n");
    for function in &metadata.functions {
        out.push_str(&format!(
            "-export([{}/{}]).\n",
            function.name, function.arity
        ));
    }
    out.push('\n');
    out.push_str("load() ->\n");
    out.push_str("    case os:getenv(\"TERLAN_SAFE_NATIVE_PATH\") of\n");
    out.push_str("        false -> ok;\n");
    out.push_str("        _Path -> ok\n");
    out.push_str("    end.\n\n");
    out.push_str("metadata() ->\n");
    out.push_str("    #{source_module => ");
    out.push_str(&erlang_binary_literal(&metadata.source_module));
    out.push_str(",\n");
    out.push_str("      native_module => ");
    out.push_str(&erlang_binary_literal(&metadata.native_module));
    out.push_str(",\n");
    out.push_str("      scheduler => ");
    out.push_str(&erlang_binary_literal(&metadata.scheduler));
    out.push_str(",\n");
    out.push_str("      operations => operations()}.\n\n");
    out.push_str("operations() ->\n");
    if metadata.functions.is_empty() {
        out.push_str("    [].\n\n");
    } else {
        out.push_str("    [");
        for (idx, function) in metadata.functions.iter().enumerate() {
            if idx > 0 {
                out.push_str(",\n     ");
            }
            let operation = function.operation.as_deref().unwrap_or(&function.name);
            out.push('{');
            out.push_str(&erlang_binary_literal(&function.name));
            out.push_str(", ");
            out.push_str(&erlang_binary_literal(operation));
            out.push_str(&format!(", {}", function.arity));
            out.push('}');
        }
        out.push_str("].\n\n");
    }
    out.push_str("start_worker(_Options) ->\n");
    out.push_str("    {error, safe_native_not_loaded_error()}.\n\n");
    out.push_str(
        "call_worker(RequestId, Operation, Args) when is_integer(RequestId), is_list(Args) ->\n",
    );
    out.push_str("    _ = Operation,\n");
    out.push_str(
        "    {safe_native_reply, RequestId, {error, safe_native_not_loaded_error()}, 0}.\n\n",
    );
    out.push_str("dispose_worker(RequestId, _Handle) when is_integer(RequestId) ->\n");
    out.push_str(
        "    {safe_native_reply, RequestId, {error, safe_native_not_loaded_error()}, 0}.\n\n",
    );
    out.push_str("stop_worker(_Bridge) ->\n");
    out.push_str("    ok.\n\n");
    out.push_str("safe_native_not_loaded_error() ->\n");
    out.push_str("    #{code => <<\"safe_native.not_loaded\">>,\n");
    out.push_str("      message => <<\"SafeNative library is not loaded.\">>,\n");
    out.push_str("      offset => 0}.\n\n");
    for function in &metadata.functions {
        let vars = (0..function.arity)
            .map(|idx| format!("A{}", idx + 1))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "{}({}) ->\n    {{error, safe_native_not_loaded_error()}}.\n\n",
            function.name, vars
        ));
    }
    out
}

/// Escapes text for an Erlang UTF-8 binary literal.
///
/// Inputs:
/// - `input`: raw metadata text.
///
/// Output:
/// - Erlang source text for a UTF-8 binary string.
///
/// Transformation:
/// - Escapes backslashes, quotes, and control characters before wrapping the
///   value in `<<"...">>`.
fn erlang_binary_literal(input: &str) -> String {
    let escaped = input
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    format!("<<\"{}\">>", escaped)
}

/// Renders a Rust SafeNative skeleton.
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
fn emit_safe_native_rust_stub(metadata: &NativeMetadata) -> String {
    let mut out = String::new();
    out.push_str("#![forbid(unsafe_code)]\n");
    out.push_str("// AUTO-GENERATED SafeNative skeleton.\n");
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
        "// Rust owns native resources. BEAM/Terlan terms should hold only opaque handles.\n",
    );
    out.push_str("#[derive(Clone, Debug, PartialEq, Eq)]\n");
    out.push_str("pub struct SafeNativeHandle {\n");
    out.push_str("    pub id: u64,\n");
    out.push_str("    pub generation: u64,\n");
    out.push_str("    pub type_name: &'static str,\n");
    out.push_str("}\n\n");
    out.push_str("#[derive(Clone, Debug, PartialEq, Eq)]\n");
    out.push_str("pub struct SafeNativeError {\n");
    out.push_str("    pub code: &'static str,\n");
    out.push_str("    pub message: String,\n");
    out.push_str("    pub offset: usize,\n");
    out.push_str("}\n\n");
    out.push_str("#[derive(Clone, Debug, PartialEq)]\n");
    out.push_str("pub enum SafeNativeValue {\n");
    out.push_str("    Unit,\n");
    out.push_str("    Text(String),\n");
    out.push_str("    Int(i64),\n");
    out.push_str("    Float(f64),\n");
    out.push_str("    Bool(bool),\n");
    out.push_str("    Handle(SafeNativeHandle),\n");
    out.push_str("    OptionalText(Option<String>),\n");
    out.push_str("    OptionalHandle(Option<SafeNativeHandle>),\n");
    out.push_str("}\n\n");
    out.push_str("#[derive(Clone, Debug, PartialEq)]\n");
    out.push_str("pub struct SafeNativeReply {\n");
    out.push_str("    pub request_id: u64,\n");
    out.push_str("    pub result: Result<SafeNativeValue, SafeNativeError>,\n");
    out.push_str("    pub credits: usize,\n");
    out.push_str("}\n\n");
    out.push_str("pub struct SafeNativeWorker {\n");
    out.push_str("    tx: Sender<SafeNativeCommand>,\n");
    out.push_str("    join: Option<JoinHandle<()>>,\n");
    out.push_str("    credit_window: usize,\n");
    out.push_str("}\n\n");
    out.push_str("enum SafeNativeCommand {\n");
    out.push_str(
        "    Register { request_id: u64, type_name: &'static str, reply: Sender<SafeNativeReply> },\n",
    );
    out.push_str(
        "    Call { request_id: u64, operation: &'static str, args: Vec<SafeNativeValue>, reply: Sender<SafeNativeReply> },\n",
    );
    out.push_str(
        "    Dispose { request_id: u64, handle: SafeNativeHandle, reply: Sender<SafeNativeReply> },\n",
    );
    out.push_str("    Stop,\n");
    out.push_str("}\n\n");
    out.push_str("impl SafeNativeWorker {\n");
    out.push_str("    pub fn start(credit_window: usize) -> Self {\n");
    out.push_str("        let credit_window = credit_window.max(1);\n");
    out.push_str("        let (tx, rx) = mpsc::channel();\n");
    out.push_str("        let join = thread::spawn(move || worker_loop(rx, credit_window));\n");
    out.push_str("        Self { tx, join: Some(join), credit_window }\n");
    out.push_str("    }\n\n");
    out.push_str("    pub fn credit_window(&self) -> usize {\n");
    out.push_str("        self.credit_window\n");
    out.push_str("    }\n\n");
    out.push_str("    pub fn register_resource(&self, request_id: u64, type_name: &'static str) -> SafeNativeReply {\n");
    out.push_str("        let (reply, rx) = mpsc::channel();\n");
    out.push_str("        self.send_and_recv(SafeNativeCommand::Register { request_id, type_name, reply }, request_id, rx)\n");
    out.push_str("    }\n\n");
    out.push_str("    pub fn call(&self, request_id: u64, operation: &'static str, args: Vec<SafeNativeValue>) -> SafeNativeReply {\n");
    out.push_str("        let (reply, rx) = mpsc::channel();\n");
    out.push_str("        self.send_and_recv(SafeNativeCommand::Call { request_id, operation, args, reply }, request_id, rx)\n");
    out.push_str("    }\n\n");
    out.push_str(
        "    pub fn dispose(&self, request_id: u64, handle: SafeNativeHandle) -> SafeNativeReply {\n",
    );
    out.push_str("        let (reply, rx) = mpsc::channel();\n");
    out.push_str(
        "        self.send_and_recv(SafeNativeCommand::Dispose { request_id, handle, reply }, request_id, rx)\n",
    );
    out.push_str("    }\n\n");
    out.push_str("    pub fn request_stop(&self) {\n");
    out.push_str("        let _ = self.tx.send(SafeNativeCommand::Stop);\n");
    out.push_str("    }\n\n");
    out.push_str("    pub fn stop(mut self) {\n");
    out.push_str("        self.request_stop();\n");
    out.push_str("        if let Some(join) = self.join.take() {\n");
    out.push_str("            let _ = join.join();\n");
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("\n");
    out.push_str("    fn send_and_recv(&self, command: SafeNativeCommand, request_id: u64, rx: Receiver<SafeNativeReply>) -> SafeNativeReply {\n");
    out.push_str("        if self.tx.send(command).is_err() {\n");
    out.push_str("            return native_error_reply(request_id, \"native_worker_stopped\", \"native worker is not accepting requests\", 0);\n");
    out.push_str("        }\n");
    out.push_str("        rx.recv().unwrap_or_else(|_| native_error_reply(request_id, \"native_worker_stopped\", \"native worker stopped before replying\", 0))\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");
    out.push_str("impl Drop for SafeNativeWorker {\n");
    out.push_str("    fn drop(&mut self) {\n");
    out.push_str("        let _ = self.tx.send(SafeNativeCommand::Stop);\n");
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
    out.push_str("fn worker_loop(rx: Receiver<SafeNativeCommand>, credit_window: usize) {\n");
    out.push_str("    let mut next_id = 1_u64;\n");
    out.push_str("    let mut resources = HashMap::<u64, ResourceState>::new();\n");
    out.push_str("    while let Ok(command) = rx.recv() {\n");
    out.push_str("        match command {\n");
    out.push_str("            SafeNativeCommand::Register { request_id, type_name, reply } => {\n");
    out.push_str("                let id = next_id;\n");
    out.push_str("                next_id += 1;\n");
    out.push_str(
        "                let handle = SafeNativeHandle { id, generation: 1, type_name };\n",
    );
    out.push_str("                resources.insert(id, ResourceState { generation: handle.generation, type_name });\n");
    out.push_str("                let _ = reply.send(SafeNativeReply { request_id, result: Ok(SafeNativeValue::Handle(handle)), credits: credit_window });\n");
    out.push_str("            }\n");
    out.push_str(
        "            SafeNativeCommand::Call { request_id, operation, args, reply } => {\n",
    );
    out.push_str("                let result = match validate_args(&resources, &args) {\n");
    out.push_str("                    Ok(()) => match operation {\n");
    for function in &metadata.functions {
        let operation = function.operation.as_deref().unwrap_or(&function.name);
        out.push_str(&format!(
            "                        \"{}\" => native_unimplemented_operation(operation),\n",
            escape_json(operation)
        ));
    }
    out.push_str("                        _ => native_unknown_operation(operation),\n");
    out.push_str("                    },\n");
    out.push_str("                    Err(err) => Err(err),\n");
    out.push_str("                };\n");
    out.push_str("                let _ = reply.send(SafeNativeReply { request_id, result, credits: credit_window });\n");
    out.push_str("            }\n");
    out.push_str("            SafeNativeCommand::Dispose { request_id, handle, reply } => {\n");
    out.push_str("                let result = match validate_handle(&resources, &handle) {\n");
    out.push_str("                    Ok(()) => {\n");
    out.push_str("                        resources.remove(&handle.id);\n");
    out.push_str("                        Ok(SafeNativeValue::Unit)\n");
    out.push_str("                    }\n");
    out.push_str("                    Err(err) => Err(err),\n");
    out.push_str("                };\n");
    out.push_str("                let _ = reply.send(SafeNativeReply { request_id, result, credits: credit_window });\n");
    out.push_str("            }\n");
    out.push_str("            SafeNativeCommand::Stop => break,\n");
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");
    out.push_str("fn native_unimplemented_operation(operation: &'static str) -> Result<SafeNativeValue, SafeNativeError> {\n");
    out.push_str("    Err(SafeNativeError { code: \"native_operation_unimplemented\", message: format!(\"native operation {} is declared but not implemented\", operation), offset: 0 })\n");
    out.push_str("}\n\n");
    out.push_str("fn native_unknown_operation(operation: &'static str) -> Result<SafeNativeValue, SafeNativeError> {\n");
    out.push_str("    Err(SafeNativeError { code: \"native_operation_unknown\", message: format!(\"native operation {} is not declared in this adapter\", operation), offset: 0 })\n");
    out.push_str("}\n\n");
    out.push_str("fn validate_args(resources: &HashMap<u64, ResourceState>, args: &[SafeNativeValue]) -> Result<(), SafeNativeError> {\n");
    out.push_str("    for arg in args {\n");
    out.push_str("        validate_value_arg(resources, arg)?;\n");
    out.push_str("    }\n");
    out.push_str("    Ok(())\n");
    out.push_str("}\n\n");
    out.push_str("fn validate_value_arg(resources: &HashMap<u64, ResourceState>, arg: &SafeNativeValue) -> Result<(), SafeNativeError> {\n");
    out.push_str("    match arg {\n");
    out.push_str(
        "        SafeNativeValue::Handle(handle) => validate_handle(resources, handle),\n",
    );
    out.push_str("        SafeNativeValue::OptionalHandle(Some(handle)) => validate_handle(resources, handle),\n");
    out.push_str("        _ => Ok(()),\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");
    out.push_str("fn validate_handle(resources: &HashMap<u64, ResourceState>, handle: &SafeNativeHandle) -> Result<(), SafeNativeError> {\n");
    out.push_str("    match resources.get(&handle.id) {\n");
    out.push_str("        Some(resource) if resource.generation == handle.generation && resource.type_name == handle.type_name => Ok(()),\n");
    out.push_str("        _ => Err(SafeNativeError { code: \"stale_native_handle\", message: format!(\"native handle {} generation {} is not live\", handle.id, handle.generation), offset: 0 }),\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");
    out.push_str("fn native_error_reply(request_id: u64, code: &'static str, message: &str, credits: usize) -> SafeNativeReply {\n");
    out.push_str("    SafeNativeReply { request_id, result: Err(SafeNativeError { code, message: message.to_string(), offset: 0 }), credits }\n");
    out.push_str("}\n");
    out
}

/// Escapes text for a quoted Erlang atom.
///
/// Inputs:
/// - `input`: raw atom text.
///
/// Output:
/// - Single-quoted Erlang atom text.
///
/// Transformation:
/// - Escapes backslashes and single quotes before wrapping the result in
///   single quotes.
fn escape_erlang_quoted_atom(input: &str) -> String {
    let escaped = input.replace('\\', "\\\\").replace('\'', "\\\'");
    format!("'{}'", escaped)
}

/// Escapes text for JSON string contents.
///
/// Inputs:
/// - `input`: raw text.
///
/// Output:
/// - Text safe to place inside a JSON string literal.
///
/// Transformation:
/// - Escapes JSON quote, slash, and control characters used by generated
///   metadata.
fn escape_json(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[cfg(test)]
#[path = "artifacts_test.rs"]
mod artifacts_test;
