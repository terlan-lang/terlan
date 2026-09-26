//! Consumer for the backend-neutral IR emitted by the Terlan self-hosted frontend.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

pub const ABI_MAJOR: u32 = 1;
pub const ABI_MINOR: u32 = 0;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrParameter {
    pub name: String,
    pub type_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrInstruction {
    pub opcode: String,
    pub result: Option<String>,
    pub operands: Vec<String>,
    pub type_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrTerminator {
    pub opcode: String,
    pub operands: Vec<String>,
    pub targets: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrBlock {
    pub label: String,
    pub instructions: Vec<IrInstruction>,
    pub terminator: IrTerminator,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrFunction {
    pub module_name: String,
    pub name: String,
    pub arity: usize,
    pub parameters: Vec<IrParameter>,
    pub return_type: String,
    pub blocks: Vec<IrBlock>,
    pub native_symbol: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrModule {
    pub name: String,
    pub functions: Vec<IrFunction>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendEnvelope {
    pub abi_major: u32,
    pub abi_minor: u32,
    pub target: String,
    pub module: IrModule,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AbiDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub function_name: Option<String>,
    pub block_label: Option<String>,
}

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
            Self::Invalid(diagnostics) => write!(
                formatter,
                "self-host IR failed structural validation with {} diagnostic(s)",
                diagnostics.len(),
            ),
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
        validate_function(&envelope.module, function, &mut diagnostics);
    }
    diagnostics
}

fn validate_function(
    module: &IrModule,
    function: &IrFunction,
    diagnostics: &mut Vec<AbiDiagnostic>,
) {
    if function.module_name != module.name {
        diagnostics.push(diagnostic(
            "function-module-mismatch",
            "function module_name must equal its containing module name",
            Some(&function.name),
            None,
        ));
    }
    if function.name.is_empty() {
        diagnostics.push(diagnostic(
            "missing-function-name",
            "function name must not be empty",
            Some(&function.name),
            None,
        ));
    }
    if function.arity != function.parameters.len() {
        diagnostics.push(diagnostic(
            "arity-parameter-mismatch",
            "function arity must equal its parameter count",
            Some(&function.name),
            None,
        ));
    }
    if function.return_type.is_empty() {
        diagnostics.push(diagnostic(
            "missing-return-type",
            "function return type must not be empty",
            Some(&function.name),
            None,
        ));
    }

    let Some(entry) = function.blocks.first() else {
        diagnostics.push(diagnostic(
            "missing-entry-block",
            "function must contain an entry block",
            Some(&function.name),
            None,
        ));
        return;
    };
    if entry.label != "entry" {
        diagnostics.push(diagnostic(
            "invalid-entry-block",
            "first function block must be named entry",
            Some(&function.name),
            Some(&entry.label),
        ));
    }

    let mut labels = HashSet::new();
    for block in &function.blocks {
        if block.label.is_empty() {
            diagnostics.push(diagnostic(
                "missing-block-label",
                "block label must not be empty",
                Some(&function.name),
                Some(&block.label),
            ));
        } else if !labels.insert(block.label.as_str()) {
            diagnostics.push(diagnostic(
                "duplicate-block-label",
                "block labels must be unique inside a function",
                Some(&function.name),
                Some(&block.label),
            ));
        }
    }

    for block in &function.blocks {
        validate_terminator(function, block, &labels, diagnostics);
        validate_instructions(function, block, &labels, diagnostics);
    }
}

fn validate_terminator(
    function: &IrFunction,
    block: &IrBlock,
    labels: &HashSet<&str>,
    diagnostics: &mut Vec<AbiDiagnostic>,
) {
    let expected_targets = match block.terminator.opcode.as_str() {
        "return" | "tail-call" | "unreachable" => Some(0),
        "jump" => Some(1),
        "branch" => Some(2),
        _ => None,
    };
    match expected_targets {
        None => diagnostics.push(diagnostic(
            "unsupported-terminator",
            "terminator is not part of self-host IR ABI 1",
            Some(&function.name),
            Some(&block.label),
        )),
        Some(expected) if block.terminator.targets.len() != expected => {
            diagnostics.push(diagnostic(
                "invalid-terminator-target-count",
                format!(
                    "{} terminator requires {expected} target(s)",
                    block.terminator.opcode
                ),
                Some(&function.name),
                Some(&block.label),
            ))
        }
        Some(_) => {}
    }
    let valid_operand_count = match block.terminator.opcode.as_str() {
        "return" | "branch" => block.terminator.operands.len() == 1,
        "jump" | "unreachable" => block.terminator.operands.is_empty(),
        "tail-call" => !block.terminator.operands.is_empty(),
        _ => true,
    };
    if !valid_operand_count {
        diagnostics.push(diagnostic(
            "invalid-terminator-operand-count",
            format!(
                "{} terminator has an invalid operand count",
                block.terminator.opcode
            ),
            Some(&function.name),
            Some(&block.label),
        ));
    }
    for target in &block.terminator.targets {
        if !labels.contains(target.as_str()) {
            diagnostics.push(diagnostic(
                "missing-block-target",
                format!("terminator target {target:?} does not exist"),
                Some(&function.name),
                Some(&block.label),
            ));
        }
    }
}

fn validate_instructions(
    function: &IrFunction,
    block: &IrBlock,
    labels: &HashSet<&str>,
    diagnostics: &mut Vec<AbiDiagnostic>,
) {
    let mut results = HashSet::new();
    for instruction in &block.instructions {
        if instruction.opcode.is_empty() {
            diagnostics.push(diagnostic(
                "missing-instruction-opcode",
                "instruction opcode must not be empty",
                Some(&function.name),
                Some(&block.label),
            ));
        }
        if !instruction_opcode_supported(&instruction.opcode) {
            diagnostics.push(diagnostic(
                "unsupported-instruction",
                format!(
                    "instruction opcode {:?} is not part of self-host IR ABI 1",
                    instruction.opcode
                ),
                Some(&function.name),
                Some(&block.label),
            ));
        } else if !instruction_operands_valid(instruction, labels) {
            diagnostics.push(diagnostic(
                "invalid-instruction-operands",
                format!("{} instruction has invalid operands", instruction.opcode),
                Some(&function.name),
                Some(&block.label),
            ));
        }
        if instruction.type_name.is_empty() {
            diagnostics.push(diagnostic(
                "missing-instruction-type",
                "instruction type must not be empty",
                Some(&function.name),
                Some(&block.label),
            ));
        }
        let Some(result) = &instruction.result else {
            diagnostics.push(diagnostic(
                "missing-instruction-result",
                "ABI 1 instructions must produce a typed result",
                Some(&function.name),
                Some(&block.label),
            ));
            continue;
        };
        if result.is_empty() || !results.insert(result.as_str()) {
            diagnostics.push(diagnostic(
                "invalid-instruction-result",
                "instruction results must be non-empty and unique inside a block",
                Some(&function.name),
                Some(&block.label),
            ));
        }
    }
}

fn instruction_opcode_supported(opcode: &str) -> bool {
    matches!(
        opcode,
        "add-small"
            | "call-import"
            | "call-local"
            | "div-small"
            | "equal"
            | "get-map-elements"
            | "get-pattern-binding"
            | "get-tuple-element"
            | "less-small"
            | "load-literal"
            | "move"
            | "mul-small"
            | "neg-small"
            | "pattern-matched"
            | "phi"
            | "prepare-pattern"
            | "put-list"
            | "put-tuple"
            | "rem-small"
            | "select"
            | "sub-small"
            | "unary-not"
    ) || opcode.starts_with("binary-")
        || opcode.starts_with("unary-")
}

fn instruction_operands_valid(instruction: &IrInstruction, labels: &HashSet<&str>) -> bool {
    let operands = &instruction.operands;
    match instruction.opcode.as_str() {
        "call-import" => operands.len() >= 2,
        "call-local" => !operands.is_empty(),
        "put-list" | "put-tuple" => true,
        "add-small" | "div-small" | "equal" | "get-tuple-element" | "less-small" | "mul-small"
        | "rem-small" | "sub-small" => operands.len() == 2,
        "get-map-elements" => operands.len() == 3,
        "get-pattern-binding" => operands.len() == 2,
        "load-literal" | "move" | "neg-small" | "pattern-matched" | "unary-not" => {
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
    let mut index = 1;
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
    if kind.is_empty() || text.is_empty() {
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
mod tests {
    use super::*;

    fn envelope(instructions: Vec<IrInstruction>, terminator: IrTerminator) -> BackendEnvelope {
        BackendEnvelope {
            abi_major: ABI_MAJOR,
            abi_minor: ABI_MINOR,
            target: "terlan-vm".to_owned(),
            module: IrModule {
                name: "probe".to_owned(),
                functions: vec![IrFunction {
                    module_name: "probe".to_owned(),
                    name: "run".to_owned(),
                    arity: 0,
                    parameters: Vec::new(),
                    return_type: "Bool".to_owned(),
                    blocks: vec![IrBlock {
                        label: "entry".to_owned(),
                        instructions,
                        terminator,
                    }],
                    native_symbol: None,
                }],
            },
        }
    }

    fn instruction(opcode: &str, operands: &[&str]) -> IrInstruction {
        IrInstruction {
            opcode: opcode.to_owned(),
            result: Some("%result".to_owned()),
            operands: operands.iter().map(|value| (*value).to_owned()).collect(),
            type_name: "Bool".to_owned(),
        }
    }

    fn return_result() -> IrTerminator {
        IrTerminator {
            opcode: "return".to_owned(),
            operands: vec!["%result".to_owned()],
            targets: Vec::new(),
        }
    }

    #[test]
    fn accepts_complete_pattern_encoding() {
        let checked = envelope(
            vec![instruction(
                "prepare-pattern",
                &[
                    "%subject",
                    "list-cons-pattern",
                    "|",
                    "2",
                    "binding-pattern",
                    "value",
                    "0",
                    "wildcard-pattern",
                    "_",
                    "0",
                ],
            )],
            return_result(),
        );
        assert!(validate(&checked).is_empty());
    }

    #[test]
    fn rejects_unknown_instruction_opcode() {
        let checked = envelope(vec![instruction("invented", &["value"])], return_result());
        assert!(validate(&checked)
            .iter()
            .any(|diagnostic| diagnostic.code == "unsupported-instruction"));
    }

    #[test]
    fn rejects_truncated_pattern_encoding() {
        let checked = envelope(
            vec![instruction(
                "prepare-pattern",
                &["%subject", "list-cons-pattern", "|", "1"],
            )],
            return_result(),
        );
        assert!(validate(&checked)
            .iter()
            .any(|diagnostic| diagnostic.code == "invalid-instruction-operands"));
    }

    #[test]
    fn rejects_invalid_terminator_operand_count() {
        let checked = envelope(
            vec![instruction("load-literal", &["true"])],
            IrTerminator {
                opcode: "return".to_owned(),
                operands: Vec::new(),
                targets: Vec::new(),
            },
        );
        assert!(validate(&checked)
            .iter()
            .any(|diagnostic| diagnostic.code == "invalid-terminator-operand-count"));
    }

    #[test]
    fn accepts_empty_aggregate_operands() {
        let checked = envelope(vec![instruction("put-list", &[])], return_result());
        assert!(validate(&checked).is_empty());
    }
}
