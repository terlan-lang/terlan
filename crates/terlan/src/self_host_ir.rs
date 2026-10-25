//! Consumer for the backend-neutral IR emitted by the Terlan self-hosted frontend.

#[path = "self_host_ir/validation.rs"]
mod validation;
use validation::*;

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

pub const ABI_MAJOR: u32 = 1;
pub const ABI_MINOR: u32 = 6;

/// Named, typed input to a self-hosted IR function.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrParameter {
    pub name: String,
    pub type_name: String,
}

/// Backend-neutral operation with an optional result binding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrInstruction {
    pub opcode: String,
    pub result: Option<String>,
    pub operands: Vec<String>,
    #[serde(default)]
    pub operand_types: Vec<Option<String>>,
    pub type_name: String,
    #[serde(default)]
    pub continuation_ordinal: Option<usize>,
    #[serde(default)]
    pub try_region: Option<IrTryRegion>,
}

/// Control-flow exit from a block, including successor labels.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrTerminator {
    pub opcode: String,
    pub operands: Vec<String>,
    pub targets: Vec<String>,
}

/// Labeled sequence of instructions ending in one terminator.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrBlock {
    pub label: String,
    pub instructions: Vec<IrInstruction>,
    pub terminator: IrTerminator,
}

/// One typed nested CFG region evaluated under a structured parent operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrRegion {
    pub label: String,
    pub parameters: Vec<IrParameter>,
    pub blocks: Vec<IrBlock>,
    pub result: String,
    pub type_name: String,
}

/// Protected, selected, and exactly-once cleanup regions of one typed `try`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrTryRegion {
    pub protected: IrRegion,
    pub success: IrRegion,
    pub failure: IrRegion,
    pub cleanup: Vec<IrRegion>,
}

/// Typed function body or native symbol declaration emitted by the frontend.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrFunction {
    pub module_name: String,
    pub name: String,
    pub arity: usize,
    #[serde(default)]
    pub capture_count: usize,
    #[serde(default)]
    pub receiver_mutable: bool,
    #[serde(default)]
    pub receiver_command: bool,
    pub parameters: Vec<IrParameter>,
    pub return_type: String,
    pub blocks: Vec<IrBlock>,
    pub native_symbol: Option<String>,
}

/// Named collection of backend-neutral function declarations.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrModule {
    pub name: String,
    #[serde(default)]
    pub structs: Vec<IrStructLayout>,
    pub functions: Vec<IrFunction>,
}

/// Named aggregate layout required to decode structural field operations.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrStructLayout {
    pub name: String,
    pub fields: Vec<IrStructField>,
}

/// One ordered aggregate field. String-only fields are accepted from ABI-1.0/1.1.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IrStructField {
    Legacy(String),
    Typed { name: String, type_name: String },
}

impl IrStructField {
    pub fn name(&self) -> &str {
        match self {
            Self::Legacy(name) | Self::Typed { name, .. } => name,
        }
    }

    pub fn type_name(&self) -> Option<&str> {
        match self {
            Self::Legacy(_) => None,
            Self::Typed { type_name, .. } => Some(type_name),
        }
    }
}

/// Versioned IR payload identifying the intended backend target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendEnvelope {
    pub abi_major: u32,
    pub abi_minor: u32,
    pub target: String,
    pub module: IrModule,
}

/// Structural validation failure with optional function and block context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AbiDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub function_name: Option<String>,
    pub block_label: Option<String>,
}

/// Failure to decode, negotiate, or structurally validate an IR envelope.
#[derive(Debug)]
pub enum AbiError {
    Decode(serde_json::Error),
    IncompatibleVersion { major: u32, minor: u32 },
    Invalid(Vec<AbiDiagnostic>),
}

impl fmt::Display for AbiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(error) => write!(formatter, "invalid self-host IR JSON: {error}"),
            Self::IncompatibleVersion { major, minor } => write!(
                formatter,
                "unsupported self-host IR ABI {major}.{minor}; consumer supports {ABI_MAJOR}.{ABI_MINOR}",
            ),
            Self::Invalid(diagnostics) => {
                write!(
                    formatter,
                    "self-host IR failed structural validation with {} diagnostic(s)",
                    diagnostics.len(),
                )?;
                for diagnostic in diagnostics {
                    write!(formatter, "; {}: {}", diagnostic.code, diagnostic.message)?;
                    if let Some(function) = &diagnostic.function_name {
                        write!(formatter, " in {function}")?;
                    }
                    if let Some(block) = &diagnostic.block_label {
                        write!(formatter, " at {block}")?;
                    }
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for AbiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Decode(error) => Some(error),
            Self::IncompatibleVersion { .. } | Self::Invalid(_) => None,
        }
    }
}

/// Decodes and validates an ABI-1 envelope before backend-specific lowering.
pub fn decode_and_validate(source: &str) -> Result<BackendEnvelope, AbiError> {
    let envelope: BackendEnvelope = serde_json::from_str(source).map_err(AbiError::Decode)?;
    if envelope.abi_major != ABI_MAJOR || envelope.abi_minor > ABI_MINOR {
        return Err(AbiError::IncompatibleVersion {
            major: envelope.abi_major,
            minor: envelope.abi_minor,
        });
    }

    let diagnostics = validate(&envelope);
    if diagnostics.is_empty() {
        Ok(envelope)
    } else {
        Err(AbiError::Invalid(diagnostics))
    }
}

/// Performs structural checks shared by VM and native backend consumers.
pub fn validate(envelope: &BackendEnvelope) -> Vec<AbiDiagnostic> {
    let mut diagnostics = Vec::new();
    if envelope.target.is_empty() {
        diagnostics.push(diagnostic(
            "missing-target",
            "backend target must not be empty",
            None,
            None,
        ));
    }
    if envelope.module.name.is_empty() {
        diagnostics.push(diagnostic(
            "missing-module-name",
            "IR module name must not be empty",
            None,
            None,
        ));
    }

    validate_struct_layouts(envelope, &mut diagnostics);

    let mut function_keys = HashSet::new();
    for function in &envelope.module.functions {
        let key = (
            function.module_name.as_str(),
            function.name.as_str(),
            function.arity,
        );
        if !function_keys.insert(key) {
            diagnostics.push(diagnostic(
                "duplicate-function",
                "function name and arity must be unique inside a module",
                Some(&function.name),
                None,
            ));
        }
        validate_function(
            &envelope.module,
            function,
            envelope.abi_minor,
            &mut diagnostics,
        );
    }
    diagnostics
}

fn validate_struct_layouts(envelope: &BackendEnvelope, diagnostics: &mut Vec<AbiDiagnostic>) {
    let mut layout_names = HashSet::new();
    for layout in &envelope.module.structs {
        if layout.name.is_empty() {
            diagnostics.push(diagnostic(
                "missing-struct-layout-name",
                "struct layout name must not be empty",
                None,
                None,
            ));
        } else if !layout_names.insert(layout.name.as_str()) {
            diagnostics.push(diagnostic(
                "duplicate-struct-layout",
                format!("struct layout {:?} is declared more than once", layout.name),
                None,
                None,
            ));
        }

        let mut field_names = HashSet::new();
        for field in &layout.fields {
            let field_name = field.name();
            if field_name.is_empty() {
                diagnostics.push(diagnostic(
                    "missing-struct-field-name",
                    format!("struct layout {:?} has an unnamed field", layout.name),
                    None,
                    None,
                ));
            } else if !field_names.insert(field_name) {
                diagnostics.push(diagnostic(
                    "duplicate-struct-field",
                    format!(
                        "struct layout {:?} declares field {:?} more than once",
                        layout.name, field_name
                    ),
                    None,
                    None,
                ));
            }

            match field.type_name() {
                Some("") => diagnostics.push(diagnostic(
                    "missing-struct-field-type",
                    format!(
                        "struct layout {:?} field {:?} has no type",
                        layout.name, field_name
                    ),
                    None,
                    None,
                )),
                None if envelope.abi_minor >= 2 => diagnostics.push(diagnostic(
                    "legacy-struct-field-in-current-abi",
                    format!(
                        "ABI 1.2 struct layout {:?} field {:?} must carry a type",
                        layout.name, field_name
                    ),
                    None,
                    None,
                )),
                Some(_) | None => {}
            }
        }
    }
}

#[cfg(test)]
#[path = "self_host_ir_struct_layout_abi_test.rs"]
mod struct_layout_abi_tests;

fn instruction_opcode_supported(opcode: &str) -> bool {
    matches!(
        opcode,
        "add-small"
            | "call-import"
            | "call-import-mut-receiver"
            | "call-local"
            | "call-local-mut-receiver"
            | "call-value"
            | "div-small"
            | "equal"
            | "get-map-elements"
            | "get-pattern-binding"
            | "get-tuple-element"
            | "less-small"
            | "load-atom"
            | "load-literal"
            | "make-closure"
            | "move"
            | "mul-small"
            | "neg-small"
            | "pattern-matched"
            | "phi"
            | "prepare-pattern"
            | "put-array"
            | "put-map"
            | "put-list"
            | "put-list-cons"
            | "put-struct"
            | "put-tuple"
            | "put-variant"
            | "rem-small"
            | "select"
            | "sub-small"
            | "try-region"
            | "update-struct-field"
            | "unary-not"
    ) || opcode.starts_with("binary-")
        || opcode.starts_with("unary-")
}

fn instruction_operands_valid(instruction: &IrInstruction, labels: &HashSet<&str>) -> bool {
    let operands = &instruction.operands;
    match instruction.opcode.as_str() {
        "call-import" | "call-import-mut-receiver" => operands.len() >= 2,
        "call-local" | "call-local-mut-receiver" => !operands.is_empty(),
        "call-value" | "make-closure" => !operands.is_empty(),
        "put-array" | "put-list" | "put-map" | "put-tuple" => true,
        "put-struct" => operands.len().is_multiple_of(2),
        "try-region" => operands.is_empty(),
        "put-variant" => operands
            .first()
            .is_some_and(|constructor| !constructor.is_empty()),
        "put-list-cons" => operands.len() >= 2,
        "add-small" | "div-small" | "equal" | "get-tuple-element" | "less-small" | "mul-small"
        | "rem-small" | "sub-small" => operands.len() == 2,
        "get-map-elements" => operands.len() == 2,
        "update-struct-field" => operands.len() == 3,
        "get-pattern-binding" => valid_pattern_binding_operands(operands),
        "load-atom" | "load-literal" | "move" | "neg-small" | "pattern-matched" | "unary-not" => {
            operands.len() == 1
        }
        "phi" => {
            operands.len() >= 2
                && operands.len().is_multiple_of(2)
                && operands
                    .iter()
                    .step_by(2)
                    .all(|label| labels.contains(label.as_str()))
        }
        "prepare-pattern" => valid_pattern_operands(operands),
        "select" => operands.len() == 3,
        opcode if opcode.starts_with("binary-") => operands.len() == 2,
        opcode if opcode.starts_with("unary-") => operands.len() == 1,
        _ => false,
    }
}

fn valid_pattern_operands(operands: &[String]) -> bool {
    if operands.len() < 4 || operands[0].is_empty() {
        return false;
    }
    let mut index = if operands.get(1).is_some_and(|value| value == "pattern-v2") {
        2
    } else {
        1
    };
    consume_pattern_node(operands, &mut index) && index == operands.len()
}

fn valid_pattern_binding_operands(operands: &[String]) -> bool {
    if operands.len() == 2 {
        return operands[1].parse::<usize>().is_ok();
    }
    if operands.len() < 6
        || operands[0].is_empty()
        || operands[1].parse::<usize>().is_err()
        || operands[2].is_empty()
    {
        return false;
    }
    let mut index = 3;
    consume_pattern_node(operands, &mut index) && index == operands.len()
}

fn consume_pattern_node(operands: &[String], index: &mut usize) -> bool {
    let Some(kind) = operands.get(*index) else {
        return false;
    };
    let Some(text) = operands.get(*index + 1) else {
        return false;
    };
    let Some(child_count) = operands
        .get(*index + 2)
        .and_then(|value| value.parse::<usize>().ok())
    else {
        return false;
    };
    if kind.is_empty() || (text.is_empty() && kind != "pattern") {
        return false;
    }
    *index += 3;
    (0..child_count).all(|_| consume_pattern_node(operands, index))
}

fn diagnostic(
    code: &'static str,
    message: impl Into<String>,
    function_name: Option<&str>,
    block_label: Option<&str>,
) -> AbiDiagnostic {
    AbiDiagnostic {
        code,
        message: message.into(),
        function_name: function_name.map(str::to_owned),
        block_label: block_label.map(str::to_owned),
    }
}

/// Stable stage digests emitted by one compiler bootstrap generation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootstrapArtifact {
    pub module_name: String,
    pub token_digest: String,
    pub syntax_digest: String,
    pub semantic_digest: String,
    pub backend_digest: String,
}

/// Per-stage equality report for two bootstrap generations.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootstrapParity {
    pub token_equal: bool,
    pub syntax_equal: bool,
    pub semantic_equal: bool,
    pub backend_equal: bool,
    pub fixed_point: bool,
    pub differences: Vec<String>,
}

/// Computes a deterministic FNV-1a digest suitable for exact bootstrap comparisons.
pub fn stable_digest(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{hash:016x}")
}

/// Serializes a checked ABI envelope and computes its deterministic backend digest.
pub fn backend_digest(envelope: &BackendEnvelope) -> Result<String, serde_json::Error> {
    serde_json::to_vec(envelope).map(|bytes| stable_digest(&bytes))
}

/// Compares all authority stages and reports whether a fixed point was reached.
pub fn compare_bootstrap_artifacts(
    reference: &BootstrapArtifact,
    candidate: &BootstrapArtifact,
) -> BootstrapParity {
    let token_equal = reference.token_digest == candidate.token_digest;
    let syntax_equal = reference.syntax_digest == candidate.syntax_digest;
    let semantic_equal = reference.semantic_digest == candidate.semantic_digest;
    let backend_equal = reference.backend_digest == candidate.backend_digest;
    let mut differences = Vec::new();
    if reference.module_name != candidate.module_name {
        differences.push("module identity differs".to_owned());
    }
    if !token_equal {
        differences.push("token artifact differs".to_owned());
    }
    if !syntax_equal {
        differences.push("syntax artifact differs".to_owned());
    }
    if !semantic_equal {
        differences.push("semantic artifact differs".to_owned());
    }
    if !backend_equal {
        differences.push("backend artifact differs".to_owned());
    }
    BootstrapParity {
        token_equal,
        syntax_equal,
        semantic_equal,
        backend_equal,
        fixed_point: differences.is_empty(),
        differences,
    }
}

#[cfg(test)]
#[path = "self_host_ir_test.rs"]
mod tests;
