//! Typed backend adapter for validated self-host ABI-1 programs.

use std::collections::HashMap;
use std::fmt;

use crate::self_host_ir::{
    decode_and_validate, validate, AbiError, BackendEnvelope, IrBlock, IrFunction, IrInstruction,
    IrParameter, IrStructLayout, IrTerminator,
};

/// Closed instruction set accepted from ABI-1 after structural validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendOpcode {
    AddSmall,
    CallImport,
    CallImportMutReceiver,
    CallLocal,
    CallLocalMutReceiver,
    CallValue,
    DivSmall,
    Equal,
    GetMapElements,
    GetPatternBinding,
    GetTupleElement,
    LessSmall,
    LoadAtom,
    LoadLiteral,
    MakeClosure,
    Move,
    MulSmall,
    NegSmall,
    PatternMatched,
    Phi,
    PreparePattern,
    PutList,
    PutListCons,
    PutTuple,
    PutVariant,
    RemSmall,
    Select,
    SubSmall,
    UnaryNot,
    Binary(String),
    Unary(String),
}

impl BackendOpcode {
    fn from_abi(opcode: &str) -> Option<Self> {
        Some(match opcode {
            "add-small" => Self::AddSmall,
            "call-import" => Self::CallImport,
            "call-import-mut-receiver" => Self::CallImportMutReceiver,
            "call-local" => Self::CallLocal,
            "call-local-mut-receiver" => Self::CallLocalMutReceiver,
            "call-value" => Self::CallValue,
            "div-small" => Self::DivSmall,
            "equal" => Self::Equal,
            "get-map-elements" => Self::GetMapElements,
            "get-pattern-binding" => Self::GetPatternBinding,
            "get-tuple-element" => Self::GetTupleElement,
            "less-small" => Self::LessSmall,
            "load-atom" => Self::LoadAtom,
            "load-literal" => Self::LoadLiteral,
            "make-closure" => Self::MakeClosure,
            "move" => Self::Move,
            "mul-small" => Self::MulSmall,
            "neg-small" => Self::NegSmall,
            "pattern-matched" => Self::PatternMatched,
            "phi" => Self::Phi,
            "prepare-pattern" => Self::PreparePattern,
            "put-list" => Self::PutList,
            "put-list-cons" => Self::PutListCons,
            "put-tuple" => Self::PutTuple,
            "put-variant" => Self::PutVariant,
            "rem-small" => Self::RemSmall,
            "select" => Self::Select,
            "sub-small" => Self::SubSmall,
            "unary-not" => Self::UnaryNot,
            value if value.starts_with("binary-") => {
                Self::Binary(value.trim_start_matches("binary-").to_owned())
            }
            value if value.starts_with("unary-") => {
                Self::Unary(value.trim_start_matches("unary-").to_owned())
            }
            _ => return None,
        })
    }
}

/// Backend operand after control-flow labels have been resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendOperand {
    Text(String),
    Block(usize),
}

/// One typed SSA instruction ready for a VM or machine-code backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendInstruction {
    pub opcode: BackendOpcode,
    pub result: String,
    pub operands: Vec<BackendOperand>,
    pub type_name: String,
}

/// Resolved control-flow exit for one backend block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendTerminator {
    Return(String),
    TailCall(Vec<String>),
    Jump(usize),
    Branch {
        condition: String,
        then_block: usize,
        else_block: usize,
    },
    Unreachable,
}

/// One block with label targets resolved to stable function-local indices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendBlock {
    pub label: String,
    pub instructions: Vec<BackendInstruction>,
    pub terminator: BackendTerminator,
}

/// One typed backend function translated from ABI-1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendFunction {
    pub module_name: String,
    pub name: String,
    pub arity: usize,
    pub capture_count: usize,
    pub parameters: Vec<IrParameter>,
    pub return_type: String,
    pub blocks: Vec<BackendBlock>,
    pub native_symbol: Option<String>,
}

/// Backend-consumable program with no unresolved control-flow labels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackendProgram {
    pub target: String,
    pub module_name: String,
    pub structs: Vec<IrStructLayout>,
    pub functions: Vec<BackendFunction>,
}

/// ABI decode/validation failure or an impossible post-validation mismatch.
#[derive(Debug)]
pub enum BackendAdapterError {
    Abi(AbiError),
    InvalidValidatedProgram(String),
}

impl fmt::Display for BackendAdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Abi(error) => error.fmt(formatter),
            Self::InvalidValidatedProgram(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for BackendAdapterError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Abi(error) => Some(error),
            Self::InvalidValidatedProgram(_) => None,
        }
    }
}

impl From<AbiError> for BackendAdapterError {
    fn from(error: AbiError) -> Self {
        Self::Abi(error)
    }
}

fn block_index(labels: &HashMap<&str, usize>, label: &str) -> Result<usize, BackendAdapterError> {
    labels.get(label).copied().ok_or_else(|| {
        BackendAdapterError::InvalidValidatedProgram(format!(
            "validated ABI-1 program references missing block {label:?}"
        ))
    })
}

fn lower_operands(
    instruction: &IrInstruction,
    labels: &HashMap<&str, usize>,
) -> Result<Vec<BackendOperand>, BackendAdapterError> {
    if instruction.opcode != "phi" {
        return Ok(instruction
            .operands
            .iter()
            .cloned()
            .map(BackendOperand::Text)
            .collect());
    }

    let mut lowered = Vec::with_capacity(instruction.operands.len());
    for pair in instruction.operands.chunks_exact(2) {
        lowered.push(BackendOperand::Block(block_index(labels, &pair[0])?));
        lowered.push(BackendOperand::Text(pair[1].clone()));
    }
    Ok(lowered)
}

fn lower_instruction(
    instruction: &IrInstruction,
    labels: &HashMap<&str, usize>,
) -> Result<BackendInstruction, BackendAdapterError> {
    let opcode = BackendOpcode::from_abi(&instruction.opcode).ok_or_else(|| {
        BackendAdapterError::InvalidValidatedProgram(format!(
            "validated ABI-1 program contains unsupported instruction {:?}",
            instruction.opcode
        ))
    })?;
    let result = instruction.result.clone().ok_or_else(|| {
        BackendAdapterError::InvalidValidatedProgram(
            "validated ABI-1 instruction has no result".to_owned(),
        )
    })?;
    Ok(BackendInstruction {
        opcode,
        result,
        operands: lower_operands(instruction, labels)?,
        type_name: instruction.type_name.clone(),
    })
}

fn lower_terminator(
    terminator: &IrTerminator,
    labels: &HashMap<&str, usize>,
) -> Result<BackendTerminator, BackendAdapterError> {
    match terminator.opcode.as_str() {
        "return" => Ok(BackendTerminator::Return(terminator.operands[0].clone())),
        "tail-call" => Ok(BackendTerminator::TailCall(terminator.operands.clone())),
        "jump" => Ok(BackendTerminator::Jump(block_index(
            labels,
            &terminator.targets[0],
        )?)),
        "branch" => Ok(BackendTerminator::Branch {
            condition: terminator.operands[0].clone(),
            then_block: block_index(labels, &terminator.targets[0])?,
            else_block: block_index(labels, &terminator.targets[1])?,
        }),
        "unreachable" => Ok(BackendTerminator::Unreachable),
        opcode => Err(BackendAdapterError::InvalidValidatedProgram(format!(
            "validated ABI-1 program contains unsupported terminator {opcode:?}"
        ))),
    }
}

fn lower_block(
    block: &IrBlock,
    labels: &HashMap<&str, usize>,
) -> Result<BackendBlock, BackendAdapterError> {
    let instructions = block
        .instructions
        .iter()
        .map(|instruction| lower_instruction(instruction, labels))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(BackendBlock {
        label: block.label.clone(),
        instructions,
        terminator: lower_terminator(&block.terminator, labels)?,
    })
}

fn lower_function(function: &IrFunction) -> Result<BackendFunction, BackendAdapterError> {
    let labels = function
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| (block.label.as_str(), index))
        .collect::<HashMap<_, _>>();
    let blocks = function
        .blocks
        .iter()
        .map(|block| lower_block(block, &labels))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(BackendFunction {
        module_name: function.module_name.clone(),
        name: function.name.clone(),
        arity: function.arity,
        capture_count: function.capture_count,
        parameters: function.parameters.clone(),
        return_type: function.return_type.clone(),
        blocks,
        native_symbol: function.native_symbol.clone(),
    })
}

/// Converts an already decoded ABI-1 envelope into a typed backend program.
pub fn lower_backend_program(
    envelope: BackendEnvelope,
) -> Result<BackendProgram, BackendAdapterError> {
    let diagnostics = validate(&envelope);
    if !diagnostics.is_empty() {
        return Err(AbiError::Invalid(diagnostics).into());
    }
    let functions = envelope
        .module
        .functions
        .iter()
        .map(lower_function)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(BackendProgram {
        target: envelope.target,
        module_name: envelope.module.name,
        structs: envelope.module.structs,
        functions,
    })
}

/// Decodes, validates, and lowers canonical ABI-1 JSON for backend consumption.
pub fn decode_backend_program(source: &str) -> Result<BackendProgram, BackendAdapterError> {
    lower_backend_program(decode_and_validate(source)?)
}

#[cfg(test)]
#[path = "self_host_backend_test.rs"]
mod tests;
