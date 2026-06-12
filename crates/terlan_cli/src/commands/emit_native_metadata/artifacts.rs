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

/// Native function export signature discovered from a native core block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeFunctionSignature {
    pub(crate) name: String,
    pub(crate) arity: usize,
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
    ///   objects.
    pub(crate) fn to_json(&self) -> String {
        let functions = self
            .functions
            .iter()
            .map(|function| {
                format!(
                    "\n    {{ \"name\": \"{}\", \"arity\": {} }}",
                    escape_json(&function.name),
                    function.arity
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
/// - `source`: Terlan source text containing a native core block.
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

    let metadata_target = out_dir.join(format!("{}.safe_nif.json", metadata.source_module));
    crate::support::write_if_changed_or_forced(
        &metadata_target,
        metadata.to_json().as_bytes(),
        incremental,
    )
    .map_err(|err| format!("failed to write native metadata: {}", err))?;

    let erl_stub_target = out_dir.join(format!("{}.erl", metadata.native_module));
    crate::support::write_if_changed_or_forced(
        &erl_stub_target,
        emit_safe_nif_erl_stub(&metadata).as_bytes(),
        incremental,
    )
    .map_err(|err| format!("failed to write native erl stub: {}", err))?;

    let rust_stub_target = out_dir.join(format!("{}.safe_nif.rs", metadata.native_module));
    let rust_stub = emit_safe_nif_rust_stub(&metadata);
    validate_safe_nif_rust_stub(&rust_stub).map_err(|err| {
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
pub(crate) fn validate_safe_nif_rust_stub(stub: &str) -> Result<(), String> {
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
/// - `Ok(NativeMetadata)` when module, native module, scheduler, and function
///   signatures are available.
/// - `Err(String)` when a required metadata field is absent.
///
/// Transformation:
/// - Scans declarations and normalizes pure policy to safe-native optional
///   when the source explicitly opts into safe native code.
pub(crate) fn extract_native_metadata(
    source: &str,
    requested_policy: NativePolicy,
) -> Result<NativeMetadata, String> {
    let source_module = extract_declared_module_name(source)
        .ok_or_else(|| "native metadata source is missing module declaration".to_string())?;
    let native_module = extract_native_module_name(source)
        .ok_or_else(|| "native metadata source is missing native module declaration".to_string())?;
    let scheduler = extract_nif_scheduler(source)
        .ok_or_else(|| "native metadata source is missing #[nif(...)] scheduler".to_string())?;
    let functions = extract_native_functions(source);
    let native_policy = if requested_policy == NativePolicy::Pure
        && source.contains("target erlang with safe_native")
    {
        NativePolicy::SafeNativeOptional
    } else {
        requested_policy
    };

    Ok(NativeMetadata {
        source_module,
        native_module,
        scheduler,
        native_policy,
        functions,
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

/// Extracts the native backend module name.
///
/// Inputs:
/// - `source`: Terlan source text.
///
/// Output:
/// - `Some(name)` for `native core module Name`.
/// - `None` when no valid native module name is found.
///
/// Transformation:
/// - Scans line by line and takes the first token after the native core module
///   prefix, stopping before whitespace or `{`.
pub(crate) fn extract_native_module_name(source: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let trimmed = line.trim();
        let rest = trimmed.strip_prefix("native core module ")?;
        let name = rest
            .split(|ch: char| ch.is_whitespace() || ch == '{')
            .next()
            .unwrap_or_default()
            .trim();
        if name.is_empty() {
            None
        } else {
            Some(name.to_string())
        }
    })
}

/// Extracts the first NIF scheduler annotation.
///
/// Inputs:
/// - `source`: Terlan source text.
///
/// Output:
/// - `Some(scheduler)` for a non-empty `#[nif(...)]` annotation.
/// - `None` when no scheduler annotation is found.
///
/// Transformation:
/// - Scans trimmed lines and strips the scheduler value from the annotation.
pub(crate) fn extract_nif_scheduler(source: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let trimmed = line.trim();
        let rest = trimmed.strip_prefix("#[nif(")?;
        rest.strip_suffix(")]")
            .map(|scheduler| scheduler.trim().to_string())
            .filter(|scheduler| !scheduler.is_empty())
    })
}

/// Extracts native function signatures from a native core block.
///
/// Inputs:
/// - `source`: Terlan source text.
///
/// Output:
/// - Function signature names and arities in source order.
///
/// Transformation:
/// - Enters the first native core module block, pairs each `#[nif(...)]`
///   annotation with the following function signature line, and skips malformed
///   signature lines.
pub(crate) fn extract_native_functions(source: &str) -> Vec<NativeFunctionSignature> {
    let mut in_block = false;
    let mut in_native_sig = false;
    let mut out = Vec::new();

    for raw_line in source.lines() {
        let trimmed = raw_line.trim();
        if trimmed.starts_with("native core module") {
            in_block = true;
            continue;
        }

        if !in_block {
            continue;
        }

        if trimmed == "}" {
            break;
        }

        if trimmed.starts_with("#[nif(") {
            in_native_sig = true;
            continue;
        }

        if in_native_sig {
            if let Some(signature) = parse_native_function_signature(trimmed) {
                out.push(signature);
            }
            in_native_sig = false;
        }
    }

    out
}

/// Parses one native function signature line.
///
/// Inputs:
/// - `line`: source line following a NIF annotation.
///
/// Output:
/// - `Some(NativeFunctionSignature)` when a name and balanced argument list
///   are found.
/// - `None` for malformed signature lines.
///
/// Transformation:
/// - Removes the trailing period, extracts the function head, and counts
///   top-level arguments.
fn parse_native_function_signature(line: &str) -> Option<NativeFunctionSignature> {
    let signature = line.trim().trim_end_matches('.').trim();
    if !signature.contains('(') || !signature.contains(')') {
        return None;
    }

    let open = signature.find('(')?;
    let close = find_matching_paren(signature, open)?;
    if close <= open + 1 {
        let name = parse_native_function_name(&signature[..open])?;
        return Some(NativeFunctionSignature { name, arity: 0 });
    }

    if close < open {
        return None;
    }

    let name = parse_native_function_name(&signature[..open])?;
    let args = &signature[open + 1..close];
    let arity = native_signature_arity(args);

    Some(NativeFunctionSignature { name, arity })
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
/// - Emits `load/0`, `-on_load`, exported NIF placeholders, and
///   `erlang:nif_error/1` bodies.
fn emit_safe_nif_erl_stub(metadata: &NativeMetadata) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "-module({}).\n",
        escape_erlang_quoted_atom(&metadata.native_module)
    ));
    out.push_str("-export([load/0]).\n");
    out.push_str("-on_load(load/0).\n");
    for function in &metadata.functions {
        out.push_str(&format!(
            "-export([{}/{}]).\n",
            function.name, function.arity
        ));
    }
    out.push('\n');
    out.push_str("load() ->\n");
    out.push_str("    case os:getenv(\"TERLAN_SAFE_NIF_PATH\") of\n");
    out.push_str("        false -> ok;\n");
    out.push_str("        Path -> erlang:load_nif(Path, 0)\n");
    out.push_str("    end.\n\n");
    for function in &metadata.functions {
        let vars = (0..function.arity)
            .map(|idx| format!("A{}", idx + 1))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "{}({}) ->\n    erlang:nif_error(nif_not_loaded).\n\n",
            function.name, vars
        ));
    }
    out
}

/// Renders a Rust SafeNative skeleton.
///
/// Inputs:
/// - `metadata`: extracted native metadata.
///
/// Output:
/// - Rust source text for a safe worker-thread skeleton.
///
/// Transformation:
/// - Emits constants for metadata and a worker object that owns its channel and
///   thread join handle without unsafe code.
fn emit_safe_nif_rust_stub(metadata: &NativeMetadata) -> String {
    let mut out = String::new();
    out.push_str("#![forbid(unsafe_code)]\n");
    out.push_str("// AUTO-GENERATED SafeNative skeleton.\n");
    out.push_str("// Implement concrete NIF exports only after adding Rust bridge code.\n\n");
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
    out.push_str("// Rust owns the worker thread. Erlang processes should hold only an opaque resource handle.\n");
    out.push_str("pub struct SafeNativeWorker {\n");
    out.push_str("    tx: Sender<SafeNativeCommand>,\n");
    out.push_str("    join: Option<JoinHandle<()>>,\n");
    out.push_str("}\n\n");
    out.push_str("enum SafeNativeCommand {\n");
    out.push_str("    Stop,\n");
    out.push_str("}\n\n");
    out.push_str("impl SafeNativeWorker {\n");
    out.push_str("    pub fn start() -> Self {\n");
    out.push_str("        let (tx, rx) = mpsc::channel();\n");
    out.push_str("        let join = thread::spawn(move || worker_loop(rx));\n");
    out.push_str("        Self { tx, join: Some(join) }\n");
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
    out.push_str("}\n\n");
    out.push_str("fn worker_loop(rx: Receiver<SafeNativeCommand>) {\n");
    out.push_str("    while let Ok(command) = rx.recv() {\n");
    out.push_str("        match command {\n");
    out.push_str("            SafeNativeCommand::Stop => break,\n");
    out.push_str("        }\n");
    out.push_str("    }\n");
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
