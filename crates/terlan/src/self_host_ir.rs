//! Consumer for the backend-neutral IR emitted by the Terlan self-hosted frontend.

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
mod struct_layout_abi_tests {
    use super::*;

    fn envelope(abi_minor: u32, field: IrStructField) -> BackendEnvelope {
        BackendEnvelope {
            abi_major: ABI_MAJOR,
            abi_minor,
            target: "native".into(),
            module: IrModule {
                name: "example.layout".into(),
                structs: vec![IrStructLayout {
                    name: "example.layout.Point".into(),
                    fields: vec![field],
                }],
                functions: Vec::new(),
            },
        }
    }

    #[test]
    fn abi_1_2_accepts_typed_struct_fields() {
        let diagnostics = validate(&envelope(
            2,
            IrStructField::Typed {
                name: "x".into(),
                type_name: "Int".into(),
            },
        ));

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn abi_1_2_rejects_legacy_struct_fields() {
        let diagnostics = validate(&envelope(2, IrStructField::Legacy("x".into())));

        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "legacy-struct-field-in-current-abi"));
    }

    #[test]
    fn abi_1_1_retains_legacy_struct_field_compatibility() {
        let diagnostics = validate(&envelope(1, IrStructField::Legacy("x".into())));

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }
}

fn validate_function(
    module: &IrModule,
    function: &IrFunction,
    abi_minor: u32,
    diagnostics: &mut Vec<AbiDiagnostic>,
) {
    if function.capture_count > function.arity || function.capture_count > function.parameters.len()
    {
        diagnostics.push(diagnostic(
            "capture-count-exceeds-arity",
            "function capture count must not exceed its arity or parameter count",
            Some(&function.name),
            None,
        ));
    }
    if abi_minor < 4 && function.capture_count != 0 {
        diagnostics.push(diagnostic(
            "capture-count-requires-abi-1.4",
            "function capture metadata requires ABI 1.4",
            Some(&function.name),
            None,
        ));
    }
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
    if function.receiver_command && !function.receiver_mutable {
        diagnostics.push(diagnostic(
            "receiver-command-without-mutable-receiver",
            "receiver command must declare a mutable receiver",
            Some(&function.name),
            None,
        ));
    }
    if function.receiver_mutable && function.parameters.is_empty() {
        diagnostics.push(diagnostic(
            "mutable-receiver-missing-parameter",
            "mutable receiver function must have a receiver parameter",
            Some(&function.name),
            None,
        ));
    }
    if function.receiver_command
        && function
            .parameters
            .first()
            .map(|parameter| parameter.type_name.as_str())
            != Some(function.return_type.as_str())
    {
        diagnostics.push(diagnostic(
            "receiver-command-return-mismatch",
            "receiver command internal return type must equal its receiver type",
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
        validate_instructions(function, block, &labels, abi_minor, diagnostics);
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
    abi_minor: u32,
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
        validate_instruction_try_region(function, block, instruction, abi_minor, diagnostics);
        if instruction.type_name.is_empty() {
            diagnostics.push(diagnostic(
                "missing-instruction-type",
                "instruction type must not be empty",
                Some(&function.name),
                Some(&block.label),
            ));
        }
        if abi_minor >= 3 && instruction.operand_types.len() != instruction.operands.len() {
            diagnostics.push(diagnostic(
                "operand-type-count-mismatch",
                format!(
                    "{} instruction has {} operand(s) but {} operand type slot(s)",
                    instruction.opcode,
                    instruction.operands.len(),
                    instruction.operand_types.len()
                ),
                Some(&function.name),
                Some(&block.label),
            ));
        }
        if instruction
            .operand_types
            .iter()
            .any(|type_name| matches!(type_name, Some(type_name) if type_name.is_empty()))
        {
            diagnostics.push(diagnostic(
                "empty-operand-type",
                "instruction operand type slots must be absent or non-empty",
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

fn validate_instruction_try_region(
    function: &IrFunction,
    block: &IrBlock,
    instruction: &IrInstruction,
    abi_minor: u32,
    diagnostics: &mut Vec<AbiDiagnostic>,
) {
    if (instruction.opcode == "try-region" || instruction.try_region.is_some()) && abi_minor < 6 {
        diagnostics.push(diagnostic(
            "try-region-requires-abi-1.6",
            "structured try regions require ABI 1.6",
            Some(&function.name),
            Some(&block.label),
        ));
    }
    match (instruction.opcode.as_str(), instruction.try_region.as_ref()) {
        ("try-region", None) => diagnostics.push(diagnostic(
            "missing-try-region",
            "try-region instruction must carry a structured region payload",
            Some(&function.name),
            Some(&block.label),
        )),
        ("try-region", Some(region)) => {
            validate_try_region(function, block, instruction, region, abi_minor, diagnostics)
        }
        (_, Some(_)) => diagnostics.push(diagnostic(
            "unexpected-try-region",
            "only try-region instructions may carry a structured region payload",
            Some(&function.name),
            Some(&block.label),
        )),
        (_, None) => {}
    }
}

fn validate_try_region(
    function: &IrFunction,
    block: &IrBlock,
    instruction: &IrInstruction,
    region: &IrTryRegion,
    abi_minor: u32,
    diagnostics: &mut Vec<AbiDiagnostic>,
) {
    let mut labels = HashSet::new();
    validate_region(
        function,
        block,
        &region.protected,
        abi_minor,
        &mut labels,
        diagnostics,
    );
    validate_region(
        function,
        block,
        &region.success,
        abi_minor,
        &mut labels,
        diagnostics,
    );
    validate_region(
        function,
        block,
        &region.failure,
        abi_minor,
        &mut labels,
        diagnostics,
    );
    for cleanup in &region.cleanup {
        validate_region(
            function,
            block,
            cleanup,
            abi_minor,
            &mut labels,
            diagnostics,
        );
    }

    if !region.protected.parameters.is_empty() {
        diagnostics.push(diagnostic(
            "invalid-try-protected-parameters",
            "protected try region must not declare parameters",
            Some(&function.name),
            Some(&block.label),
        ));
    }
    if region.success.parameters.len() != 1
        || region.success.parameters[0].type_name != region.protected.type_name
    {
        diagnostics.push(diagnostic(
            "invalid-try-success-parameters",
            "success region must accept exactly the protected result type",
            Some(&function.name),
            Some(&block.label),
        ));
    }
    if region.failure.parameters.len() != 1 || region.failure.parameters[0].type_name != "Int" {
        diagnostics.push(diagnostic(
            "invalid-try-failure-parameters",
            "failure region must accept exactly one Int status parameter",
            Some(&function.name),
            Some(&block.label),
        ));
    }
    if region
        .cleanup
        .iter()
        .any(|cleanup| !cleanup.parameters.is_empty() || cleanup.type_name != "Unit")
    {
        diagnostics.push(diagnostic(
            "invalid-try-cleanup-contract",
            "cleanup regions must have no parameters and Unit result type",
            Some(&function.name),
            Some(&block.label),
        ));
    }
    if region.success.type_name != instruction.type_name
        || region.failure.type_name != instruction.type_name
    {
        diagnostics.push(diagnostic(
            "try-region-result-type-mismatch",
            "success and failure region types must equal the try instruction type",
            Some(&function.name),
            Some(&block.label),
        ));
    }
}

fn validate_region<'a>(
    function: &IrFunction,
    parent: &IrBlock,
    region: &'a IrRegion,
    abi_minor: u32,
    region_labels: &mut HashSet<&'a str>,
    diagnostics: &mut Vec<AbiDiagnostic>,
) {
    if region.label.is_empty() {
        diagnostics.push(diagnostic(
            "invalid-try-region-label",
            "try region label must not be empty",
            Some(&function.name),
            Some(&parent.label),
        ));
    } else if !region_labels.insert(region.label.as_str()) {
        diagnostics.push(diagnostic(
            "duplicate-try-region-label",
            "try region labels must be unique inside one try instruction",
            Some(&function.name),
            Some(&parent.label),
        ));
    }
    if region.type_name.is_empty() {
        diagnostics.push(diagnostic(
            "missing-region-type",
            "try region result type must not be empty",
            Some(&function.name),
            Some(&parent.label),
        ));
    }
    if region.result.is_empty() {
        diagnostics.push(diagnostic(
            "missing-region-result",
            "try region result identity must not be empty",
            Some(&function.name),
            Some(&parent.label),
        ));
    }
    let Some(entry) = region.blocks.first() else {
        diagnostics.push(diagnostic(
            "missing-region-block",
            "try region must contain an entry block",
            Some(&function.name),
            Some(&parent.label),
        ));
        return;
    };
    if entry.label != "entry" {
        diagnostics.push(diagnostic(
            "invalid-region-entry",
            "first try region block must be named entry",
            Some(&function.name),
            Some(&parent.label),
        ));
    }
    let mut labels = HashSet::new();
    for nested in &region.blocks {
        if nested.label.is_empty() || !labels.insert(nested.label.as_str()) {
            diagnostics.push(diagnostic(
                "invalid-region-block-label",
                "try region block labels must be non-empty and unique",
                Some(&function.name),
                Some(&parent.label),
            ));
        }
    }
    for nested in &region.blocks {
        validate_terminator(function, nested, &labels, diagnostics);
        validate_instructions(function, nested, &labels, abi_minor, diagnostics);
    }
    let bound = region
        .parameters
        .iter()
        .any(|parameter| parameter.name == region.result)
        || region.blocks.iter().any(|nested| {
            nested
                .instructions
                .iter()
                .any(|instruction| instruction.result.as_deref() == Some(region.result.as_str()))
        });
    if !bound {
        diagnostics.push(diagnostic(
            "unbound-region-result",
            "try region result must name a parameter or instruction result",
            Some(&function.name),
            Some(&parent.label),
        ));
    }
}

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
