//! Fail-closed ABI-1 to NativeIR adapter for the first self-hosted scalar slice.

use std::collections::{HashMap, HashSet};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use crate::runtime::native_image::managed::{encode_closure_allocation, ManagedClosureDescriptor};
use crate::self_host_ir::ABI_MINOR;
use crate::self_host_ir::{
    BackendEnvelope, IrBlock, IrFunction, IrInstruction, IrRegion, IrStructField, IrTryRegion,
};
#[cfg(test)]
use crate::self_host_ir::{IrParameter, IrTerminator};

use super::function_lowering::{NativeFunction, NativeModule};
use super::identity::stable_export_id;
use super::model::{NativeBinaryOperator, NativeExpr, NativeType};

type ValueCatalog = HashMap<String, (usize, NativeType)>;

#[derive(Clone, Debug)]
struct LocalFunction {
    index: usize,
    capture_count: usize,
    parameters: Vec<NativeType>,
    result: NativeType,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LocalStructField {
    pub(super) index: usize,
    pub(super) type_name: String,
}

struct FunctionCatalog {
    callables: HashMap<(String, String, usize), LocalFunction>,
    structs: HashMap<String, HashMap<String, LocalStructField>>,
    variants: Option<crate::self_host_variant_ir::VariantCatalog>,
}

impl FunctionCatalog {
    fn new() -> Self {
        Self {
            callables: HashMap::new(),
            structs: HashMap::new(),
            variants: None,
        }
    }
}

impl Deref for FunctionCatalog {
    type Target = HashMap<(String, String, usize), LocalFunction>;

    fn deref(&self) -> &Self::Target {
        &self.callables
    }
}

impl DerefMut for FunctionCatalog {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.callables
    }
}

const MAX_APPLICATION_MODULES: usize = 4_096;
const MAX_APPLICATION_FUNCTIONS: usize = 1_000_000;
const MAX_MODULE_FUNCTIONS: usize = 65_536;
const MAX_FUNCTION_BLOCKS: usize = 65_536;
const MAX_FUNCTION_INSTRUCTIONS: usize = 1_000_000;
const MAX_CFG_EXPANSIONS: usize = 4_000_000;
const MAX_INSTRUCTION_OPERANDS: usize = 65_536;
const MAX_IDENTITY_BYTES: usize = 4_096;

fn adapter_error(context: &str, message: impl AsRef<str>) -> String {
    format!(
        "error[self-host.native-adapter.{context}]: {}",
        message.as_ref()
    )
}

fn native_type(type_name: &str) -> Result<NativeType, String> {
    if let Some(native) = super::self_host_managed_layouts::native_type(type_name)? {
        return Ok(native);
    }
    match type_name {
        "Unit" => Ok(NativeType::Unit),
        "Int" => Ok(NativeType::Int),
        "Float" => Ok(NativeType::Float),
        "Bool" => Ok(NativeType::Bool),
        "Atom" => Ok(NativeType::Atom),
        "String" => Ok(NativeType::StringRef),
        "Bytes" => Ok(NativeType::BytesRef),
        "Binary" => Ok(NativeType::BinaryRef),
        "" | "Dynamic" => Err(adapter_error(
            "type",
            "unresolved or dynamic types cannot enter direct native lowering",
        )),
        other => Err(adapter_error(
            "type",
            format!("managed type `{other}` does not yet have an ABI-1 layout adapter"),
        )),
    }
}

fn map_type_arguments(type_name: &str) -> Result<(&str, &str), String> {
    let source = type_name.trim();
    let opening = source
        .find('[')
        .ok_or_else(|| adapter_error("map_construct", format!("`{source}` is not a Map type")))?;
    let constructor = source[..opening].trim();
    if constructor.rsplit('.').next() != Some("Map") || !source.ends_with(']') {
        return Err(adapter_error(
            "map_construct",
            format!("`{source}` is not a Map type"),
        ));
    }
    let arguments = &source[opening + 1..source.len() - 1];
    let mut depth = 0_usize;
    for (index, byte) in arguments.bytes().enumerate() {
        match byte {
            b'[' | b'{' | b'(' => depth = depth.saturating_add(1),
            b']' | b'}' | b')' => {
                depth = depth.checked_sub(1).ok_or_else(|| {
                    adapter_error("map_construct", "Map type has unbalanced delimiters")
                })?;
            }
            b',' if depth == 0 => {
                let key = arguments[..index].trim();
                let value = arguments[index + 1..].trim();
                if key.is_empty() || value.is_empty() {
                    break;
                }
                return Ok((key, value));
            }
            _ => {}
        }
    }
    Err(adapter_error(
        "map_construct",
        format!("Map type `{source}` must have key and value arguments"),
    ))
}

pub(super) fn decoded_string_literal(text: &str) -> Result<String, String> {
    if !text.starts_with('"') {
        return Ok(text.to_owned());
    }
    let inner = text
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| adapter_error("literal", "unterminated String literal"))?;
    let mut output = String::new();
    let mut chars = inner.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        let escaped = chars
            .next()
            .ok_or_else(|| adapter_error("literal", "unterminated String escape"))?;
        output.push(match escaped {
            '"' => '"',
            '\\' => '\\',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            other => other,
        });
    }
    Ok(output)
}

fn map_construct_expr(
    instruction: &IrInstruction,
    values: &ValueCatalog,
) -> Result<(NativeExpr, NativeType), String> {
    if !instruction.operands.len().is_multiple_of(2) {
        return Err(adapter_error(
            "map_construct",
            "put-map requires alternating key/value operands",
        ));
    }
    if instruction.operand_types.len() != instruction.operands.len() {
        return Err(adapter_error(
            "map_construct",
            "put-map requires type evidence for every operand",
        ));
    }
    let result_type = native_type(&instruction.type_name)?;
    let NativeType::ManagedRef(semantic) = result_type else {
        return Err(adapter_error(
            "map_construct",
            "put-map result must have a managed Map type",
        ));
    };
    let (key_name, value_name) = map_type_arguments(&instruction.type_name)?;
    let key_type = native_type(key_name)?;
    let value_type = native_type(value_name)?;
    let mut args = Vec::with_capacity(instruction.operands.len());
    for (index, operand) in instruction.operands.iter().enumerate() {
        let (value, producer_type) = value_operand(operand, values)?;
        let declared_name = instruction.operand_types[index]
            .as_deref()
            .ok_or_else(|| adapter_error("map_construct", "map operand type is unresolved"))?;
        let declared_type = native_type(declared_name)?;
        let expected = if index.is_multiple_of(2) {
            key_type
        } else {
            value_type
        };
        if declared_type != expected || producer_type != expected {
            return Err(adapter_error(
                "map_construct",
                format!("map operand {index} disagrees with its declared Map type"),
            ));
        }
        args.push(value);
    }
    Ok((
        NativeExpr::ManagedOperation {
            encoded: Arc::from(
                crate::runtime::native_image::managed::encode_map_from_entries_operation(semantic),
            ),
            args,
        },
        result_type,
    ))
}

#[cfg(test)]
mod map_construct_tests {
    use super::*;

    fn instruction(operands: &[&str], operand_types: &[&str]) -> IrInstruction {
        IrInstruction {
            opcode: "put-map".to_owned(),
            result: Some("%map".to_owned()),
            operands: operands.iter().map(|value| (*value).to_owned()).collect(),
            operand_types: operand_types
                .iter()
                .map(|value| Some((*value).to_owned()))
                .collect(),
            type_name: "Map[String, Int]".to_owned(),
            continuation_ordinal: None,
            try_region: None,
        }
    }

    #[test]
    fn map_construct_accepts_exact_alternating_type_evidence() {
        let mut values = ValueCatalog::default();
        values.insert("%key".to_owned(), (0, NativeType::StringRef));
        values.insert("%value".to_owned(), (1, NativeType::Int));

        let (expression, result_type) = map_construct_expr(
            &instruction(&["%key", "%value"], &["String", "Int"]),
            &values,
        )
        .expect("typed map construction");

        assert!(matches!(result_type, NativeType::ManagedRef(_)));
        assert!(matches!(
            expression,
            NativeExpr::ManagedOperation { args, .. } if args.len() == 2
        ));
    }

    #[test]
    fn map_construct_rejects_odd_operands_with_positive_companion() {
        map_construct_accepts_exact_alternating_type_evidence();
        let values = ValueCatalog::default();
        let error = map_construct_expr(&instruction(&["%key"], &["String"]), &values)
            .expect_err("odd map operands must fail");
        assert!(error.contains("alternating key/value operands"));
    }
}

fn value_operand(name: &str, values: &ValueCatalog) -> Result<(NativeExpr, NativeType), String> {
    values
        .get(name)
        .map(|(index, value_type)| (NativeExpr::Param(*index), *value_type))
        .ok_or_else(|| adapter_error("operand", format!("value `{name}` has no producer")))
}

fn one_operand<'a>(instruction: &'a IrInstruction) -> Result<&'a str, String> {
    match instruction.operands.as_slice() {
        [operand] => Ok(operand),
        _ => Err(adapter_error(
            "instruction",
            format!(
                "opcode `{}` requires exactly one operand",
                instruction.opcode
            ),
        )),
    }
}

fn two_operands<'a>(instruction: &'a IrInstruction) -> Result<(&'a str, &'a str), String> {
    match instruction.operands.as_slice() {
        [left, right] => Ok((left, right)),
        _ => Err(adapter_error(
            "instruction",
            format!(
                "opcode `{}` requires exactly two operands",
                instruction.opcode
            ),
        )),
    }
}

fn literal(
    instruction: &IrInstruction,
    result_type: NativeType,
    atoms: &mut Vec<String>,
) -> Result<NativeExpr, String> {
    let text = one_operand(instruction)?;
    match result_type {
        NativeType::Unit if text == "Unit" || text == "unit" || text == "()" => {
            Ok(NativeExpr::Unit)
        }
        NativeType::Int => text
            .parse::<i64>()
            .map(NativeExpr::Int)
            .map_err(|_| adapter_error("literal", format!("invalid Int literal `{text}`"))),
        NativeType::Float => text
            .parse::<f64>()
            .map(|value| NativeExpr::Float(value.to_bits()))
            .map_err(|_| adapter_error("literal", format!("invalid Float literal `{text}`"))),
        NativeType::Bool if text == "true" => Ok(NativeExpr::Bool(true)),
        NativeType::Bool if text == "false" => Ok(NativeExpr::Bool(false)),
        NativeType::Atom => {
            if !atoms.iter().any(|atom| atom == text) {
                atoms.push(text.to_owned());
            }
            Ok(NativeExpr::AtomLiteral(Arc::<str>::from(text)))
        }
        NativeType::StringRef => Ok(NativeExpr::ManagedLiteral {
            encoded: Arc::from(
                crate::runtime::native_image::managed::encode_string_literal(
                    &decoded_string_literal(text)?,
                )
                .map_err(|error| adapter_error("literal", error.to_string()))?,
            ),
        }),
        NativeType::BytesRef | NativeType::BinaryRef => Err(adapter_error(
            "literal",
            "managed scalar literals require canonical managed encoding",
        )),
        _ => Err(adapter_error(
            "literal",
            format!(
                "literal `{text}` does not match `{}`",
                instruction.type_name
            ),
        )),
    }
}

fn binary_operator(opcode: &str) -> Option<NativeBinaryOperator> {
    match opcode {
        "add-small" | "binary-+" => Some(NativeBinaryOperator::Add),
        "subtract-small" | "sub-small" | "binary--" => Some(NativeBinaryOperator::Subtract),
        "multiply-small" | "mul-small" | "binary-*" => Some(NativeBinaryOperator::Multiply),
        "divide-small" | "div-small" | "binary-/" => Some(NativeBinaryOperator::Divide),
        "remainder-small" | "rem-small" | "binary-%" => Some(NativeBinaryOperator::Remainder),
        "equal" | "equal-exact" | "binary-==" => Some(NativeBinaryOperator::Equal),
        "not-equal-exact" | "binary-!=" => Some(NativeBinaryOperator::NotEqual),
        "less-small" | "binary-<" => Some(NativeBinaryOperator::LessThan),
        "less-equal-small" | "binary-<=" => Some(NativeBinaryOperator::LessThanOrEqual),
        "greater-small" | "binary->" => Some(NativeBinaryOperator::GreaterThan),
        "greater-equal-small" | "binary->=" => Some(NativeBinaryOperator::GreaterThanOrEqual),
        _ => None,
    }
}

fn binary_expression(
    instruction: &IrInstruction,
    operator: NativeBinaryOperator,
    result_type: NativeType,
    values: &ValueCatalog,
) -> Result<NativeExpr, String> {
    let (left_name, right_name) = two_operands(instruction)?;
    let (left, left_type) = value_operand(left_name, values)?;
    let (right, right_type) = value_operand(right_name, values)?;
    if left_type != right_type {
        return Err(adapter_error(
            "binary",
            format!(
                "operands of `{}` have different native types",
                instruction.opcode
            ),
        ));
    }
    let comparison = matches!(
        operator,
        NativeBinaryOperator::Equal
            | NativeBinaryOperator::NotEqual
            | NativeBinaryOperator::LessThan
            | NativeBinaryOperator::LessThanOrEqual
            | NativeBinaryOperator::GreaterThan
            | NativeBinaryOperator::GreaterThanOrEqual
    );
    if comparison && result_type != NativeType::Bool {
        return Err(adapter_error(
            "binary",
            format!("comparison `{}` must produce Bool", instruction.opcode),
        ));
    }
    if !comparison && result_type != left_type {
        return Err(adapter_error(
            "binary",
            format!(
                "arithmetic `{}` changes its operand type",
                instruction.opcode
            ),
        ));
    }
    if !matches!(left_type, NativeType::Int | NativeType::Float) {
        return Err(adapter_error(
            "binary",
            format!(
                "opcode `{}` has unsupported native operands",
                instruction.opcode
            ),
        ));
    }
    Ok(NativeExpr::Binary {
        operator,
        operand_type: left_type,
        left: Box::new(left),
        right: Box::new(right),
    })
}

fn local_call(
    instruction: &IrInstruction,
    module_name: &str,
    result_type: NativeType,
    values: &ValueCatalog,
    functions: &FunctionCatalog,
) -> Result<NativeExpr, String> {
    let (callee, arguments) = instruction
        .operands
        .split_first()
        .ok_or_else(|| adapter_error("call", "local call has no callable identity"))?;
    invoke_call(
        module_name,
        callee,
        arguments,
        result_type,
        values,
        functions,
    )
}

fn invoke_call(
    module_name: &str,
    callee: &str,
    arguments: &[String],
    result_type: NativeType,
    values: &ValueCatalog,
    functions: &FunctionCatalog,
) -> Result<NativeExpr, String> {
    let target = functions
        .get(&(module_name.to_owned(), callee.to_owned(), arguments.len()))
        .ok_or_else(|| {
            adapter_error(
                "call",
                format!(
                    "local callable `{callee}/{}` is not in this ABI module",
                    arguments.len()
                ),
            )
        })?;
    if target.result != result_type {
        return Err(adapter_error(
            "call",
            format!(
                "local call `{callee}/{}` has the wrong result type",
                arguments.len()
            ),
        ));
    }
    let typed_args = arguments
        .iter()
        .map(|argument| value_operand(argument, values))
        .collect::<Result<Vec<_>, _>>()?;
    if typed_args
        .iter()
        .map(|(_, value_type)| *value_type)
        .ne(target.parameters.iter().copied())
    {
        return Err(adapter_error(
            "call",
            format!(
                "local call `{callee}/{}` has incompatible arguments",
                arguments.len()
            ),
        ));
    }
    Ok(NativeExpr::Call {
        function: target.index,
        args: typed_args
            .into_iter()
            .map(|(expression, _)| expression)
            .collect(),
    })
}

fn imported_call(
    instruction: &IrInstruction,
    result_type: NativeType,
    values: &ValueCatalog,
    functions: &FunctionCatalog,
) -> Result<NativeExpr, String> {
    match instruction.operands.as_slice() {
        [module_name, callee, arguments @ ..] => invoke_call(
            module_name,
            callee,
            arguments,
            result_type,
            values,
            functions,
        ),
        _ => Err(adapter_error(
            "call",
            "imported call has no module or callable identity",
        )),
    }
}

fn local_mutable_call(
    instruction: &IrInstruction,
    module_name: &str,
    result_type: NativeType,
    values: &ValueCatalog,
    functions: &FunctionCatalog,
) -> Result<NativeExpr, String> {
    match instruction.operands.as_slice() {
        [callee, _place, arguments @ ..] => invoke_call(
            module_name,
            callee,
            arguments,
            result_type,
            values,
            functions,
        ),
        _ => Err(adapter_error(
            "call",
            "mutable receiver call has no callable, place, or receiver operand",
        )),
    }
}

fn imported_mutable_call(
    instruction: &IrInstruction,
    result_type: NativeType,
    values: &ValueCatalog,
    functions: &FunctionCatalog,
) -> Result<NativeExpr, String> {
    match instruction.operands.as_slice() {
        [module_name, callee, _place, arguments @ ..] => invoke_call(
            module_name,
            callee,
            arguments,
            result_type,
            values,
            functions,
        ),
        _ => Err(adapter_error(
            "call",
            "imported mutable call has no module, callable, place, or receiver operand",
        )),
    }
}

fn managed_instruction_operands(
    instruction: &IrInstruction,
    values: &ValueCatalog,
) -> Result<Option<Vec<NativeExpr>>, String> {
    if !matches!(
        instruction.opcode.as_str(),
        "put-array"
            | "put-list"
            | "put-list-cons"
            | "put-tuple"
            | "put-variant"
            | "get-tuple-element"
    ) {
        return Ok(None);
    }
    if instruction.operand_types.len() != instruction.operands.len() {
        return Err(adapter_error(
            "managed_operand_types",
            format!(
                "opcode `{}` requires one ABI-1.3 type slot per operand",
                instruction.opcode
            ),
        ));
    }
    let mut args = Vec::with_capacity(instruction.operands.len());
    for (index, operand) in instruction.operands.iter().enumerate() {
        if instruction.opcode == "put-variant" && index == 0 {
            if instruction.operand_types[index].is_some() || operand.trim().is_empty() {
                return Err(adapter_error(
                    "variant_constructor",
                    "put-variant constructor metadata must be nonempty and untyped",
                ));
            }
            continue;
        }
        if instruction.opcode == "get-tuple-element" && index == 1 {
            let value = operand.parse::<i64>().map_err(|_| {
                adapter_error(
                    "tuple_projection_index",
                    format!("tuple index `{operand}` is not a non-negative integer"),
                )
            })?;
            if value < 0 {
                return Err(adapter_error(
                    "tuple_projection_index",
                    format!("tuple index `{operand}` is negative"),
                ));
            }
            args.push(NativeExpr::Int(value));
            continue;
        }
        let (value, actual_type) = value_operand(operand, values)?;
        let expected_type = instruction.operand_types[index]
            .as_deref()
            .ok_or_else(|| {
                adapter_error(
                    "managed_operand_type",
                    format!(
                        "opcode `{}` operand {index} has no resolved type",
                        instruction.opcode
                    ),
                )
            })
            .and_then(native_type)?;
        if actual_type != expected_type {
            return Err(adapter_error(
                "managed_operand_type",
                format!(
                    "opcode `{}` operand {index} type evidence disagrees with its SSA producer",
                    instruction.opcode
                ),
            ));
        }
        args.push(value);
    }
    Ok(Some(args))
}

fn closure_type(parameters: &[NativeType], result: NativeType) -> Result<NativeType, String> {
    let parameters = parameters
        .iter()
        .copied()
        .map(NativeType::boundary_type)
        .collect::<Vec<_>>();
    ManagedClosureDescriptor::semantic_id_for_signature(&parameters, &[result.boundary_type()])
        .map(NativeType::ManagedRef)
        .map_err(|error| adapter_error("closure_signature", error.to_string()))
}

fn make_closure(
    instruction: &IrInstruction,
    module_name: &str,
    values: &ValueCatalog,
    functions: &FunctionCatalog,
) -> Result<(NativeExpr, NativeType), String> {
    let (callee, captures) = instruction
        .operands
        .split_first()
        .ok_or_else(|| adapter_error("closure", "make-closure has no callable identity"))?;
    let candidates = functions
        .iter()
        .filter(|((module, name, _arity), target)| {
            module == module_name && name == callee && target.capture_count == captures.len()
        })
        .collect::<Vec<_>>();
    if candidates.len() != 1 {
        return Err(adapter_error(
            "closure_target",
            format!(
                "callable `{module_name}.{callee}` is absent or overloaded without an ABI signature"
            ),
        ));
    }
    let (identity, target) = candidates[0];
    let capture_values = captures
        .iter()
        .map(|capture| value_operand(capture, values))
        .collect::<Result<Vec<_>, _>>()?;
    if capture_values
        .iter()
        .map(|(_, capture_type)| *capture_type)
        .ne(target.parameters[..target.capture_count].iter().copied())
    {
        return Err(adapter_error(
            "closure_captures",
            "captured values disagree with the lifted callable prefix",
        ));
    }
    let result_type = closure_type(&target.parameters[target.capture_count..], target.result)?;
    let callable_id = stable_export_id(module_name, callee, identity.2);
    let encoded = encode_closure_allocation(callable_id)
        .map_err(|error| adapter_error("closure_allocation", error.to_string()))?;
    Ok((
        NativeExpr::MakeClosure {
            encoded: Arc::from(encoded),
            captures: capture_values
                .into_iter()
                .map(|(capture, _)| capture)
                .collect(),
        },
        result_type,
    ))
}

fn call_value(
    instruction: &IrInstruction,
    result_type: NativeType,
    values: &ValueCatalog,
) -> Result<NativeExpr, String> {
    let (callee_name, arguments) = instruction
        .operands
        .split_first()
        .ok_or_else(|| adapter_error("closure_call", "call-value has no callee"))?;
    let (callee, callee_type) = value_operand(callee_name, values)?;
    let typed_args = arguments
        .iter()
        .map(|argument| value_operand(argument, values))
        .collect::<Result<Vec<_>, _>>()?;
    let parameter_types = typed_args
        .iter()
        .map(|(_, argument_type)| *argument_type)
        .collect::<Vec<_>>();
    let expected_closure = closure_type(&parameter_types, result_type)?;
    if callee_type != expected_closure {
        return Err(adapter_error(
            "closure_call",
            "call-value callee signature disagrees with its typed arguments or result",
        ));
    }
    Ok(NativeExpr::InvokeClosure {
        callee: Box::new(callee),
        args: typed_args
            .into_iter()
            .map(|(argument, _)| argument)
            .collect(),
        parameter_types,
        result_type,
    })
}

fn region_has_continuation(region: &IrRegion) -> bool {
    region.blocks.iter().any(|block| {
        block.instructions.iter().any(|instruction| {
            instruction.continuation_ordinal.is_some()
                || instruction
                    .try_region
                    .as_ref()
                    .is_some_and(|nested| try_region_has_continuation(nested))
        })
    })
}

fn try_region_has_continuation(region: &IrTryRegion) -> bool {
    region_has_continuation(&region.protected)
        || region_has_continuation(&region.success)
        || region_has_continuation(&region.failure)
        || region.cleanup.iter().any(region_has_continuation)
}

fn validate_region_return_identity(region: &IrRegion) -> Result<(), String> {
    let mut returns = 0_usize;
    for block in &region.blocks {
        if block.terminator.opcode != "return" {
            continue;
        }
        returns += 1;
        match block.terminator.operands.as_slice() {
            [result] if result == &region.result => {}
            _ => {
                return Err(adapter_error(
                    "try_region",
                    format!(
                        "region `{}` must return its declared result `{}`",
                        region.label, region.result
                    ),
                ));
            }
        }
    }
    if returns == 0 {
        return Err(adapter_error(
            "try_region",
            format!("region `{}` has no returning block", region.label),
        ));
    }
    Ok(())
}

fn region_values(
    region: &IrRegion,
    outer: &ValueCatalog,
    slots: &[usize],
) -> Result<ValueCatalog, String> {
    if region.parameters.len() != slots.len() {
        return Err(adapter_error(
            "try_region",
            format!(
                "region `{}` parameter count disagrees with its native slots",
                region.label
            ),
        ));
    }
    let mut values = outer.clone();
    for (parameter, slot) in region.parameters.iter().zip(slots.iter().copied()) {
        if values.contains_key(&parameter.name) {
            return Err(adapter_error(
                "try_region",
                format!(
                    "region `{}` parameter `{}` shadows an outer value",
                    region.label, parameter.name
                ),
            ));
        }
        values.insert(
            parameter.name.clone(),
            (slot, native_type(&parameter.type_name)?),
        );
    }
    Ok(values)
}

fn lower_region(
    region: &IrRegion,
    slots: &[usize],
    expected_type: NativeType,
    module_name: &str,
    outer: &ValueCatalog,
    functions: &FunctionCatalog,
    atoms: &mut Vec<String>,
    expansions: &mut usize,
) -> Result<NativeExpr, String> {
    let declared_type = native_type(&region.type_name)?;
    if declared_type != expected_type {
        return Err(adapter_error(
            "try_region",
            format!(
                "region `{}` type disagrees with its structured role",
                region.label
            ),
        ));
    }
    validate_region_return_identity(region)?;
    let actor_sites: &[super::self_host_actor_adapter::NativeActorSuspensionSite] = &[];
    lower_block_path(
        "entry",
        None,
        &region.blocks,
        module_name,
        region_values(region, outer, slots)?,
        functions,
        actor_sites,
        expected_type,
        atoms,
        &HashSet::new(),
        expansions,
    )?
    .ok_or_else(|| {
        adapter_error(
            "try_region",
            format!(
                "region `{}` has no returning control-flow path",
                region.label
            ),
        )
    })
}

fn one_parameter_type(region: &IrRegion, expected: &str) -> bool {
    matches!(region.parameters.as_slice(), [parameter] if parameter.type_name == expected)
}

fn try_region_expression(
    instruction: &IrInstruction,
    module_name: &str,
    values: &ValueCatalog,
    functions: &FunctionCatalog,
    atoms: &mut Vec<String>,
) -> Result<(NativeExpr, NativeType), String> {
    if !instruction.operands.is_empty() {
        return Err(adapter_error(
            "try_region",
            "try-region cannot carry flat operands",
        ));
    }
    let regions = instruction.try_region.as_ref().ok_or_else(|| {
        adapter_error("try_region", "try-region has no structured region payload")
    })?;
    if try_region_has_continuation(regions) {
        return Err(adapter_error(
            "try_region",
            "structured try regions cannot contain actor continuations",
        ));
    }
    if !regions.protected.parameters.is_empty() {
        return Err(adapter_error(
            "try_region",
            "protected region cannot declare parameters",
        ));
    }
    if !one_parameter_type(&regions.success, &regions.protected.type_name) {
        return Err(adapter_error(
            "try_region",
            "success region must accept exactly the protected result type",
        ));
    }
    if !one_parameter_type(&regions.failure, "Int") {
        return Err(adapter_error(
            "try_region",
            "failure region must accept exactly one Int status",
        ));
    }
    if regions
        .cleanup
        .iter()
        .any(|region| !region.parameters.is_empty() || region.type_name != "Unit")
    {
        return Err(adapter_error(
            "try_region",
            "cleanup regions must accept no parameters and return Unit",
        ));
    }

    let result_type = native_type(&instruction.type_name)?;
    let protected_type = native_type(&regions.protected.type_name)?;
    let value_slot = values
        .values()
        .map(|(slot, _)| *slot)
        .max()
        .map_or(0, |slot| slot + 1);
    let mut expansions = 0_usize;
    let protected = lower_region(
        &regions.protected,
        &[],
        protected_type,
        module_name,
        values,
        functions,
        atoms,
        &mut expansions,
    )?;
    let success = lower_region(
        &regions.success,
        &[value_slot],
        result_type,
        module_name,
        values,
        functions,
        atoms,
        &mut expansions,
    )?;
    let failure = lower_region(
        &regions.failure,
        &[value_slot],
        result_type,
        module_name,
        values,
        functions,
        atoms,
        &mut expansions,
    )?;
    let cleanup = regions
        .cleanup
        .iter()
        .map(|region| {
            lower_region(
                region,
                &[],
                NativeType::Unit,
                module_name,
                values,
                functions,
                atoms,
                &mut expansions,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((
        NativeExpr::Try {
            protected: Box::new(protected),
            success: Box::new(success),
            failure: Box::new(failure),
            cleanup,
        },
        result_type,
    ))
}

fn instruction_expression(
    instruction: &IrInstruction,
    module_name: &str,
    values: &ValueCatalog,
    functions: &FunctionCatalog,
    atoms: &mut Vec<String>,
) -> Result<(NativeExpr, NativeType), String> {
    if instruction.opcode == "try-region" {
        return try_region_expression(instruction, module_name, values, functions, atoms);
    }
    if instruction.opcode == "prepare-pattern" {
        let (subject_name, _pattern) =
            super::self_host_pattern_ir::decode_prepare_pattern(instruction)?;
        let (subject, subject_type) = value_operand(subject_name, values)?;
        return super::self_host_managed_layouts::prepare_pattern_expr_with_catalogs(
            instruction,
            subject,
            subject_type,
            functions.variants.as_ref(),
            Some(&functions.structs),
        );
    }
    if matches!(
        instruction.opcode.as_str(),
        "pattern-matched" | "get-pattern-binding"
    ) {
        let environment_name = instruction.operands.first().ok_or_else(|| {
            adapter_error(
                "pattern_environment",
                format!("opcode `{}` has no environment operand", instruction.opcode),
            )
        })?;
        let (environment, environment_type) = value_operand(environment_name, values)?;
        return super::self_host_managed_layouts::pattern_environment_expr_with_catalogs(
            instruction,
            environment,
            environment_type,
            functions.variants.as_ref(),
            Some(&functions.structs),
        )?
        .ok_or_else(|| {
            adapter_error(
                "pattern_environment",
                format!("opcode `{}` has no pattern lowering", instruction.opcode),
            )
        });
    }
    if instruction.opcode == "make-closure" {
        return make_closure(instruction, module_name, values, functions);
    }
    let result_type = native_type(&instruction.type_name)?;
    if instruction.opcode == "get-map-elements" {
        let source_name = instruction
            .operands
            .first()
            .ok_or_else(|| adapter_error("map_field", "get-map-elements has no source operand"))?;
        let (source, source_type) = value_operand(source_name, values)?;
        let declared_source = instruction
            .operand_types
            .first()
            .and_then(Option::as_deref)
            .ok_or_else(|| adapter_error("struct_field", "field source has no type evidence"))?;
        let constructor = declared_source
            .split_once('[')
            .map_or(declared_source, |(name, _)| name)
            .trim();
        if let Some(fields) = functions.structs.get(constructor) {
            let field_name = instruction.operands.get(1).ok_or_else(|| {
                adapter_error("struct_field", "field projection has no field identity")
            })?;
            let field = fields.get(field_name).ok_or_else(|| {
                adapter_error(
                    "struct_field",
                    format!("struct `{constructor}` has no field `{field_name}`"),
                )
            })?;
            let field_type = native_type(&field.type_name)?;
            if field_type != result_type {
                return Err(adapter_error(
                    "struct_field",
                    "field result type disagrees with its ABI layout",
                ));
            }
            return super::self_host_managed_layouts::aggregate_field_expr(
                instruction,
                source,
                source_type,
                result_type,
                field.index,
            );
        }
        return super::self_host_managed_layouts::map_field_expr(
            instruction,
            source,
            source_type,
            result_type,
        );
    }
    if instruction.opcode == "update-struct-field" {
        let [source_name, field_name, replacement_name] = instruction.operands.as_slice() else {
            return Err(adapter_error(
                "struct_update",
                "update-struct-field requires source, field, and replacement operands",
            ));
        };
        let (source, source_type) = value_operand(source_name, values)?;
        let (replacement, replacement_type) = value_operand(replacement_name, values)?;
        let declared_source = instruction
            .operand_types
            .first()
            .and_then(Option::as_deref)
            .ok_or_else(|| adapter_error("struct_update", "field source has no type evidence"))?;
        let constructor = declared_source
            .split_once('[')
            .map_or(declared_source, |(name, _)| name)
            .trim();
        let fields = functions.structs.get(constructor).ok_or_else(|| {
            adapter_error(
                "struct_update",
                format!("struct `{constructor}` has no admitted ABI layout"),
            )
        })?;
        let field = fields.get(field_name).ok_or_else(|| {
            adapter_error(
                "struct_update",
                format!("struct `{constructor}` has no field `{field_name}`"),
            )
        })?;
        return super::self_host_managed_layouts::aggregate_replace_field_expr(
            instruction,
            source,
            source_type,
            replacement,
            replacement_type,
            result_type,
            field.index,
            &field.type_name,
        );
    }
    if instruction.opcode == "put-map" {
        return map_construct_expr(instruction, values);
    }
    if instruction.opcode == "put-struct" {
        let constructor = instruction
            .type_name
            .split_once('[')
            .map_or(instruction.type_name.as_str(), |(name, _)| name)
            .trim();
        if constructor
            .strip_prefix(module_name)
            .and_then(|suffix| suffix.strip_prefix('.'))
            .is_none()
        {
            return Err(adapter_error(
                "struct_construct_visibility",
                format!("cannot raw-construct non-local struct `{constructor}`"),
            ));
        }
        let declared = functions.structs.get(constructor).ok_or_else(|| {
            adapter_error(
                "struct_construct",
                format!("struct `{constructor}` has no admitted ABI layout"),
            )
        })?;
        if instruction.operands.len() != instruction.operand_types.len()
            || !instruction.operands.len().is_multiple_of(2)
        {
            return Err(adapter_error(
                "struct_construct",
                "put-struct requires aligned field/value operand pairs",
            ));
        }
        let mut provided = HashMap::new();
        for (pair_index, pair) in instruction.operands.chunks_exact(2).enumerate() {
            let name = &pair[0];
            let (value, value_type) = value_operand(&pair[1], values)?;
            let declared_name_type = instruction
                .operand_types
                .get(pair_index * 2)
                .and_then(Option::as_deref)
                .ok_or_else(|| {
                    adapter_error("struct_construct", "field identity has no type evidence")
                })?;
            if native_type(declared_name_type)? != native_type("String")? {
                return Err(adapter_error(
                    "struct_construct",
                    "field identity must carry String type evidence",
                ));
            }
            let declared_value_type = instruction
                .operand_types
                .get(pair_index * 2 + 1)
                .and_then(Option::as_deref)
                .ok_or_else(|| {
                    adapter_error("struct_construct", "field value has no type evidence")
                })?;
            if native_type(declared_value_type)? != value_type {
                return Err(adapter_error(
                    "struct_construct",
                    "field value type evidence disagrees with its SSA producer",
                ));
            }
            if provided.insert(name.clone(), (value, value_type)).is_some() {
                return Err(adapter_error(
                    "struct_construct",
                    format!("struct `{constructor}` repeats field `{name}`"),
                ));
            }
        }
        if provided.len() != declared.len() {
            return Err(adapter_error(
                "struct_construct",
                format!("struct `{constructor}` requires every declared field"),
            ));
        }
        let mut ordered = declared.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|(_, field)| field.index);
        let mut fields = Vec::with_capacity(ordered.len());
        for (name, field) in ordered {
            let (value, actual) = provided.remove(name).ok_or_else(|| {
                adapter_error(
                    "struct_construct",
                    format!("struct `{constructor}` is missing field `{name}`"),
                )
            })?;
            let expected = native_type(&field.type_name)?;
            if actual != expected {
                return Err(adapter_error(
                    "struct_construct",
                    format!("struct `{constructor}` field `{name}` has the wrong type"),
                ));
            }
            fields.push((name.clone(), field.type_name.clone(), value));
        }
        return Ok((
            super::self_host_managed_layouts::aggregate_record_expr(
                &instruction.type_name,
                fields,
            )?,
            result_type,
        ));
    }
    if let Some(args) = managed_instruction_operands(instruction, values)? {
        let expression = super::self_host_managed_layouts::managed_instruction_expr_with_variants(
            instruction,
            args,
            functions.variants.as_ref(),
        )?
        .ok_or_else(|| {
            adapter_error(
                "managed_instruction",
                format!("opcode `{}` has no managed lowering", instruction.opcode),
            )
        })?;
        return Ok((expression, result_type));
    }
    let expression = match instruction.opcode.as_str() {
        "load-literal" => literal(instruction, result_type, atoms)?,
        "load-atom" if result_type == NativeType::Atom => {
            literal(instruction, NativeType::Atom, atoms)?
        }
        "load-atom" => {
            return Err(adapter_error("literal", "load-atom must produce Atom"));
        }
        "move" => {
            let (value, value_type) = value_operand(one_operand(instruction)?, values)?;
            if value_type != result_type {
                return Err(adapter_error("move", "move changes the source value type"));
            }
            value
        }
        "neg-small" => {
            let (value, value_type) = value_operand(one_operand(instruction)?, values)?;
            if value_type != result_type {
                return Err(adapter_error(
                    "negation",
                    "negation changes the operand type",
                ));
            }
            match value_type {
                NativeType::Int => NativeExpr::Neg(Box::new(value)),
                NativeType::Float => NativeExpr::FloatNeg(Box::new(value)),
                _ => {
                    return Err(adapter_error(
                        "negation",
                        "negation requires an Int or Float operand",
                    ))
                }
            }
        }
        "unary-not" | "unary-!" => {
            let (value, value_type) = value_operand(one_operand(instruction)?, values)?;
            if value_type != NativeType::Bool || result_type != NativeType::Bool {
                return Err(adapter_error(
                    "not",
                    "logical negation requires a Bool operand",
                ));
            }
            NativeExpr::Not(Box::new(value))
        }
        "call-local" => local_call(instruction, module_name, result_type, values, functions)?,
        "call-import" => imported_call(instruction, result_type, values, functions)?,
        "call-local-mut-receiver" => {
            local_mutable_call(instruction, module_name, result_type, values, functions)?
        }
        "call-import-mut-receiver" => {
            imported_mutable_call(instruction, result_type, values, functions)?
        }
        "call-value" => call_value(instruction, result_type, values)?,
        opcode => match binary_operator(opcode) {
            Some(operator) => binary_expression(instruction, operator, result_type, values)?,
            None => {
                return Err(adapter_error(
                    "opcode",
                    format!("opcode `{opcode}` is outside the direct-native scalar slice"),
                ))
            }
        },
    };
    Ok((expression, result_type))
}

#[cfg(test)]
mod try_region_tests {
    use super::*;

    fn literal(result: &str, value: &str, type_name: &str) -> IrInstruction {
        IrInstruction {
            opcode: "load-literal".to_owned(),
            result: Some(result.to_owned()),
            operands: vec![value.to_owned()],
            operand_types: vec![None],
            type_name: type_name.to_owned(),
            continuation_ordinal: None,
            try_region: None,
        }
    }

    fn region(
        label: &str,
        parameters: Vec<IrParameter>,
        result: &str,
        type_name: &str,
        instructions: Vec<IrInstruction>,
    ) -> IrRegion {
        IrRegion {
            label: label.to_owned(),
            parameters,
            blocks: vec![IrBlock {
                label: "entry".to_owned(),
                instructions,
                terminator: IrTerminator {
                    opcode: "return".to_owned(),
                    operands: vec![result.to_owned()],
                    targets: vec![],
                },
            }],
            result: result.to_owned(),
            type_name: type_name.to_owned(),
        }
    }

    fn instruction(success_parameters: Vec<IrParameter>) -> IrInstruction {
        IrInstruction {
            opcode: "try-region".to_owned(),
            result: Some("%result".to_owned()),
            operands: vec![],
            operand_types: vec![],
            type_name: "Int".to_owned(),
            continuation_ordinal: None,
            try_region: Some(IrTryRegion {
                protected: region(
                    "protected",
                    vec![],
                    "%protected",
                    "Int",
                    vec![literal("%protected", "1", "Int")],
                ),
                success: region("success", success_parameters, "%value", "Int", vec![]),
                failure: region(
                    "failure",
                    vec![IrParameter {
                        name: "%status".to_owned(),
                        type_name: "Int".to_owned(),
                    }],
                    "%status",
                    "Int",
                    vec![],
                ),
                cleanup: vec![region(
                    "cleanup",
                    vec![],
                    "%unit",
                    "Unit",
                    vec![literal("%unit", "Unit", "Unit")],
                )],
            }),
        }
    }

    #[test]
    fn structured_try_lowers_to_native_try() {
        let expression = try_region_expression(
            &instruction(vec![IrParameter {
                name: "%value".to_owned(),
                type_name: "Int".to_owned(),
            }]),
            "sample",
            &ValueCatalog::default(),
            &FunctionCatalog::new(),
            &mut vec![],
        )
        .expect("valid structured try");
        assert_eq!(expression.1, NativeType::Int);
        assert!(matches!(
            expression.0,
            NativeExpr::Try { cleanup, .. } if cleanup.len() == 1
        ));
    }

    #[test]
    fn structured_try_rejects_invalid_success_role_with_positive_companion() {
        structured_try_lowers_to_native_try();
        let error = try_region_expression(
            &instruction(vec![]),
            "sample",
            &ValueCatalog::default(),
            &FunctionCatalog::new(),
            &mut vec![],
        )
        .expect_err("missing success parameter must fail");
        assert!(error.contains("success region must accept exactly"));
    }
}

fn wrap_bindings(bindings: Vec<NativeExpr>, body: NativeExpr) -> NativeExpr {
    if bindings.is_empty() {
        body
    } else {
        NativeExpr::Let {
            bindings,
            body: Box::new(body),
        }
    }
}

fn phi_operand<'a>(instruction: &'a IrInstruction, predecessor: &str) -> Result<&'a str, String> {
    if instruction.operands.len() % 2 != 0 {
        return Err(adapter_error(
            "phi",
            "phi operands must alternate predecessor labels and values",
        ));
    }
    instruction
        .operands
        .chunks_exact(2)
        .find(|pair| pair[0] == predecessor)
        .map(|pair| pair[1].as_str())
        .ok_or_else(|| {
            adapter_error(
                "phi",
                format!("phi has no value for predecessor `{predecessor}`"),
            )
        })
}

fn result_name<'a>(instruction: &'a IrInstruction) -> Result<&'a str, String> {
    instruction.result.as_deref().ok_or_else(|| {
        adapter_error(
            "instruction",
            format!("opcode `{}` has no result", instruction.opcode),
        )
    })
}

fn bind_block_instructions(
    block: &IrBlock,
    predecessor: Option<&str>,
    module_name: &str,
    values: &mut ValueCatalog,
    functions: &FunctionCatalog,
    actor_sites: &[super::self_host_actor_adapter::NativeActorSuspensionSite],
    atoms: &mut Vec<String>,
) -> Result<Vec<NativeExpr>, String> {
    let mut bindings = Vec::with_capacity(block.instructions.len());
    let mut saw_non_phi = false;
    for instruction in &block.instructions {
        let result = result_name(instruction)?;
        if values.contains_key(result) {
            return Err(adapter_error(
                "instruction",
                format!("result `{result}` already has a producer"),
            ));
        }
        let (mut expression, result_type) = if instruction.opcode == "phi" {
            if saw_non_phi {
                return Err(adapter_error(
                    "phi",
                    "phi instructions must lead their block",
                ));
            }
            let predecessor = predecessor.ok_or_else(|| {
                adapter_error("phi", "entry block cannot contain a phi instruction")
            })?;
            let source = phi_operand(instruction, predecessor)?;
            let (expression, source_type) = value_operand(source, values)?;
            let result_type = native_type(&instruction.type_name)?;
            if source_type != result_type {
                return Err(adapter_error(
                    "phi",
                    format!("phi result `{result}` changes its incoming value type"),
                ));
            }
            (expression, result_type)
        } else {
            saw_non_phi = true;
            instruction_expression(instruction, module_name, values, functions, atoms)?
        };
        let matching_sites = actor_sites
            .iter()
            .filter(|site| site.block_label == block.label && site.result_name == result)
            .collect::<Vec<_>>();
        if matching_sites.len() > 1 {
            return Err(adapter_error(
                "actor-site",
                format!(
                    "block `{}` result `{result}` has duplicate suspension sites",
                    block.label
                ),
            ));
        }
        if let Some(site) = matching_sites.first() {
            let captures = site
                .value_names
                .iter()
                .map(|name| value_operand(name, values).map(|(value, _)| value))
                .collect::<Result<Vec<_>, _>>()?;
            expression =
                super::self_host_actor_adapter::apply_suspension_site(expression, site, captures)?;
        }
        let index = values.len();
        values.insert(result.to_owned(), (index, result_type));
        bindings.push(expression);
    }
    Ok(bindings)
}

fn block_named<'a>(blocks: &'a [IrBlock], label: &str) -> Result<&'a IrBlock, String> {
    blocks
        .iter()
        .find(|block| block.label == label)
        .ok_or_else(|| adapter_error("control-flow", format!("block `{label}` does not exist")))
}

fn returned_value(
    block: &IrBlock,
    values: &ValueCatalog,
    return_type: NativeType,
) -> Result<NativeExpr, String> {
    match block.terminator.operands.as_slice() {
        [] if return_type == NativeType::Unit => Ok(NativeExpr::Unit),
        [value] => {
            let (expression, value_type) = value_operand(value, values)?;
            if value_type != return_type {
                return Err(adapter_error(
                    "return",
                    format!("block `{}` returns a value of the wrong type", block.label),
                ));
            }
            Ok(expression)
        }
        _ => Err(adapter_error(
            "return",
            format!("block `{}` has an invalid return operand", block.label),
        )),
    }
}

fn lower_block_path(
    label: &str,
    predecessor: Option<&str>,
    blocks: &[IrBlock],
    module_name: &str,
    mut values: ValueCatalog,
    functions: &FunctionCatalog,
    actor_sites: &[super::self_host_actor_adapter::NativeActorSuspensionSite],
    return_type: NativeType,
    atoms: &mut Vec<String>,
    path: &HashSet<String>,
    expansions: &mut usize,
) -> Result<Option<NativeExpr>, String> {
    *expansions = expansions
        .checked_add(1)
        .ok_or_else(|| adapter_error("resource", "CFG expansion count overflow"))?;
    if *expansions > MAX_CFG_EXPANSIONS {
        return Err(adapter_error(
            "resource",
            format!("CFG exceeds the {MAX_CFG_EXPANSIONS} expansion bound"),
        ));
    }
    if path.contains(label) {
        return Err(adapter_error(
            "control-flow",
            format!("cyclic CFG reaches block `{label}`; loop structuring is not implemented"),
        ));
    }
    let mut next_path = path.clone();
    next_path.insert(label.to_owned());
    let block = block_named(blocks, label)?;
    let bindings = bind_block_instructions(
        block,
        predecessor,
        module_name,
        &mut values,
        functions,
        actor_sites,
        atoms,
    )?;
    if actor_sites
        .iter()
        .any(|site| site.block_label == block.label)
    {
        let mut bindings = bindings;
        let terminal = bindings.pop().ok_or_else(|| {
            adapter_error(
                "actor-site",
                format!(
                    "split suspension block `{}` has no call binding",
                    block.label
                ),
            )
        })?;
        return Ok(Some(wrap_bindings(bindings, terminal)));
    }
    let body = match block.terminator.opcode.as_str() {
        "return" => Some(returned_value(block, &values, return_type)?),
        "unreachable" => None,
        "jump" => match block.terminator.targets.as_slice() {
            [target] => lower_block_path(
                target,
                Some(&block.label),
                blocks,
                module_name,
                values,
                functions,
                actor_sites,
                return_type,
                atoms,
                &next_path,
                expansions,
            )?,
            _ => {
                return Err(adapter_error(
                    "control-flow",
                    format!("jump in block `{}` does not have one target", block.label),
                ))
            }
        },
        "branch" => {
            let condition_name = match block.terminator.operands.as_slice() {
                [condition] => condition,
                _ => {
                    return Err(adapter_error(
                        "control-flow",
                        format!(
                            "branch in block `{}` does not have one condition",
                            block.label
                        ),
                    ))
                }
            };
            let (condition, condition_type) = value_operand(condition_name, &values)?;
            if condition_type != NativeType::Bool {
                return Err(adapter_error(
                    "control-flow",
                    format!(
                        "branch in block `{}` does not use a Bool condition",
                        block.label
                    ),
                ));
            }
            let (true_target, false_target) = match block.terminator.targets.as_slice() {
                [true_target, false_target] => (true_target, false_target),
                _ => {
                    return Err(adapter_error(
                        "control-flow",
                        format!(
                            "branch in block `{}` does not have two targets",
                            block.label
                        ),
                    ))
                }
            };
            let true_body = lower_block_path(
                true_target,
                Some(&block.label),
                blocks,
                module_name,
                values.clone(),
                functions,
                actor_sites,
                return_type,
                atoms,
                &next_path,
                expansions,
            )?;
            let false_body = lower_block_path(
                false_target,
                Some(&block.label),
                blocks,
                module_name,
                values,
                functions,
                actor_sites,
                return_type,
                atoms,
                &next_path,
                expansions,
            )?;
            match (true_body, false_body) {
                (Some(selected), Some(fallback)) => Some(NativeExpr::If {
                    clauses: vec![(condition, selected), (NativeExpr::Bool(true), fallback)],
                }),
                (Some(selected), None) => Some(NativeExpr::If {
                    clauses: vec![(condition, selected)],
                }),
                (None, Some(fallback)) => Some(NativeExpr::If {
                    clauses: vec![(NativeExpr::Not(Box::new(condition)), fallback)],
                }),
                (None, None) => None,
            }
        }
        opcode => {
            return Err(adapter_error(
                "control-flow",
                format!("terminator `{opcode}` is outside acyclic CFG lowering"),
            ))
        }
    };
    Ok(body.map(|expression| wrap_bindings(bindings, expression)))
}

fn native_function(
    module_name: &str,
    function: &IrFunction,
    params: Vec<NativeType>,
    return_type: NativeType,
    body: NativeExpr,
) -> NativeFunction {
    NativeFunction {
        export_id: stable_export_id(module_name, &function.name, function.arity),
        name: function.name.clone(),
        public: true,
        arity: function.arity,
        source_module: module_name.to_owned(),
        source_function: function.name.clone(),
        source_arity: function.arity,
        callable_captures: params[..function.capture_count].to_vec(),
        params,
        return_type,
        body,
    }
}

fn lower_cfg_function(
    module_name: &str,
    function: &IrFunction,
    functions: &FunctionCatalog,
    actor_sites: &[super::self_host_actor_adapter::NativeActorSuspensionSite],
    atoms: &mut Vec<String>,
) -> Result<NativeFunction, String> {
    let params = function
        .parameters
        .iter()
        .map(|parameter| native_type(&parameter.type_name))
        .collect::<Result<Vec<_>, _>>()?;
    if params.len() != function.arity {
        return Err(adapter_error(
            "arity",
            format!(
                "`{}.{}` parameter count does not match arity",
                module_name, function.name
            ),
        ));
    }
    let values = function
        .parameters
        .iter()
        .zip(params.iter().copied())
        .enumerate()
        .map(|(index, (parameter, value_type))| (parameter.name.clone(), (index, value_type)))
        .collect::<ValueCatalog>();
    let return_type = native_type(&function.return_type)?;
    let mut expansions = 0;
    let body = lower_block_path(
        "entry",
        None,
        &function.blocks,
        module_name,
        values,
        functions,
        actor_sites,
        return_type,
        atoms,
        &HashSet::new(),
        &mut expansions,
    )?
    .ok_or_else(|| {
        adapter_error(
            "control-flow",
            format!(
                "`{}.{}` has no returning control-flow path",
                module_name, function.name
            ),
        )
    })?;
    Ok(native_function(
        module_name,
        function,
        params,
        return_type,
        body,
    ))
}

fn lower_function_with_actor_sites(
    module_name: &str,
    function: &IrFunction,
    functions: &FunctionCatalog,
    actor_sites: &[super::self_host_actor_adapter::NativeActorSuspensionSite],
    atoms: &mut Vec<String>,
) -> Result<NativeFunction, String> {
    validate_actor_sites(function, actor_sites)?;
    if function.receiver_command && !function.receiver_mutable {
        return Err(adapter_error(
            "receiver",
            format!(
                "`{}.{}` is a receiver command without a mutable receiver",
                module_name, function.name
            ),
        ));
    }
    if function.receiver_mutable && function.parameters.is_empty() {
        return Err(adapter_error(
            "receiver",
            format!(
                "`{}.{}` has no receiver parameter",
                module_name, function.name
            ),
        ));
    }
    if function.receiver_command
        && function
            .parameters
            .first()
            .map(|parameter| parameter.type_name.as_str())
            != Some(function.return_type.as_str())
    {
        return Err(adapter_error(
            "receiver",
            format!(
                "`{}.{}` command return does not match its receiver",
                module_name, function.name
            ),
        ));
    }
    if function.native_symbol.is_some() {
        return Err(adapter_error(
            "native-symbol",
            format!(
                "`{}.{}` delegates to a native symbol",
                module_name, function.name
            ),
        ));
    }
    if function.blocks.len() != 1 || !actor_sites.is_empty() {
        return lower_cfg_function(module_name, function, functions, actor_sites, atoms);
    }
    let [block] = function.blocks.as_slice() else {
        return Err(adapter_error(
            "control-flow",
            format!(
                "`{}.{}` is not a single-block function",
                module_name, function.name
            ),
        ));
    };
    if block.label != "entry" || block.terminator.opcode != "return" {
        return Err(adapter_error(
            "control-flow",
            format!(
                "`{}.{}` is not an entry/return scalar function",
                module_name, function.name
            ),
        ));
    }
    let params = function
        .parameters
        .iter()
        .map(|parameter| native_type(&parameter.type_name))
        .collect::<Result<Vec<_>, _>>()?;
    if params.len() != function.arity {
        return Err(adapter_error(
            "arity",
            format!(
                "`{}.{}` parameter count does not match arity",
                module_name, function.name
            ),
        ));
    }
    let mut values = function
        .parameters
        .iter()
        .zip(params.iter().copied())
        .enumerate()
        .map(|(index, (parameter, value_type))| (parameter.name.clone(), (index, value_type)))
        .collect::<ValueCatalog>();
    let mut bindings = Vec::with_capacity(block.instructions.len());
    for instruction in &block.instructions {
        let result = instruction.result.as_ref().ok_or_else(|| {
            adapter_error(
                "instruction",
                format!(
                    "opcode `{}` has no result in scalar lowering",
                    instruction.opcode
                ),
            )
        })?;
        if values.contains_key(result) {
            return Err(adapter_error(
                "instruction",
                format!("result `{result}` already has a producer"),
            ));
        }
        let (mut expression, result_type) =
            instruction_expression(instruction, module_name, &values, functions, atoms)?;
        let matching_sites = actor_sites
            .iter()
            .filter(|site| site.block_label == block.label && site.result_name == *result)
            .collect::<Vec<_>>();
        if matching_sites.len() > 1 {
            return Err(adapter_error(
                "actor-site",
                format!(
                    "block `{}` result `{result}` has duplicate suspension sites",
                    block.label
                ),
            ));
        }
        if let Some(site) = matching_sites.first() {
            let captures = site
                .value_names
                .iter()
                .map(|name| value_operand(name, &values).map(|(value, _)| value))
                .collect::<Result<Vec<_>, _>>()?;
            expression =
                super::self_host_actor_adapter::apply_suspension_site(expression, site, captures)?;
        }
        let index = params.len() + bindings.len();
        values.insert(result.clone(), (index, result_type));
        bindings.push(expression);
    }
    let return_type = native_type(&function.return_type)?;
    let body = match block.terminator.operands.as_slice() {
        [] if function.return_type == "Unit" => NativeExpr::Unit,
        [value] => {
            let (expression, value_type) = value_operand(value, &values)?;
            if value_type != return_type {
                return Err(adapter_error(
                    "return",
                    format!(
                        "`{}.{}` returns a value of the wrong type",
                        module_name, function.name
                    ),
                ));
            }
            expression
        }
        _ => {
            return Err(adapter_error(
                "return",
                format!(
                    "`{}.{}` has an invalid return operand",
                    module_name, function.name
                ),
            ))
        }
    };
    let body = wrap_bindings(bindings, body);
    Ok(native_function(
        module_name,
        function,
        params,
        return_type,
        body,
    ))
}

fn call_instruction(opcode: &str) -> bool {
    matches!(
        opcode,
        "call-local"
            | "call-import"
            | "call-local-mut-receiver"
            | "call-import-mut-receiver"
            | "call-value"
    )
}

fn validate_actor_sites(
    function: &IrFunction,
    actor_sites: &[super::self_host_actor_adapter::NativeActorSuspensionSite],
) -> Result<(), String> {
    let calls = function
        .blocks
        .iter()
        .flat_map(|block| {
            block
                .instructions
                .iter()
                .filter(|instruction| call_instruction(&instruction.opcode))
                .map(move |instruction| (block.label.as_str(), instruction))
        })
        .collect::<Vec<_>>();
    let mut anchors = HashSet::new();
    let mut indexes = HashSet::new();
    for site in actor_sites {
        if site.source_function != function.name || site.source_arity != function.arity {
            return Err(adapter_error(
                "actor-site-owner",
                format!(
                    "site `{}:{}` does not belong to `{} {}/{}`",
                    site.block_label,
                    site.result_name,
                    function.name,
                    function.name,
                    function.arity
                ),
            ));
        }
        if !anchors.insert((site.block_label.clone(), site.result_name.clone())) {
            return Err(adapter_error(
                "actor-site",
                format!(
                    "function `{}/{}` repeats suspension anchor `{}:{}`",
                    function.name, function.arity, site.block_label, site.result_name
                ),
            ));
        }
        if !indexes.insert(site.native_call_index) {
            return Err(adapter_error(
                "actor-call-index",
                format!(
                    "function `{}/{}` repeats native call index {}",
                    function.name, function.arity, site.native_call_index
                ),
            ));
        }
        let Some((block, instruction)) = calls.get(site.native_call_index) else {
            return Err(adapter_error(
                "actor-call-index",
                format!(
                    "function `{}/{}` has no native call at index {}",
                    function.name, function.arity, site.native_call_index
                ),
            ));
        };
        if *block != site.block_label
            || instruction.result.as_deref() != Some(site.result_name.as_str())
        {
            return Err(adapter_error(
                "actor-call-anchor",
                format!(
                    "native call index {} resolves to `{}:{}` instead of `{}:{}`",
                    site.native_call_index,
                    block,
                    instruction.result.as_deref().unwrap_or("<missing>"),
                    site.block_label,
                    site.result_name
                ),
            ));
        }
        let owner_block = function
            .blocks
            .iter()
            .find(|block| block.label == site.block_label)
            .ok_or_else(|| {
                adapter_error(
                    "actor-call-anchor",
                    format!("suspension block `{}` does not exist", site.block_label),
                )
            })?;
        let is_last = owner_block
            .instructions
            .last()
            .and_then(|instruction| instruction.result.as_deref())
            == Some(site.result_name.as_str());
        if !is_last || owner_block.terminator.opcode != "unreachable" {
            return Err(adapter_error(
                "actor-continuation-split",
                format!(
                    "suspending call `{}:{}` must end an unreachable split block",
                    site.block_label, site.result_name
                ),
            ));
        }
    }
    Ok(())
}

fn lower_function(
    module_name: &str,
    function: &IrFunction,
    functions: &FunctionCatalog,
    atoms: &mut Vec<String>,
) -> Result<NativeFunction, String> {
    lower_function_with_actor_sites(module_name, function, functions, &[], atoms)
}

/// Converts one admitted ABI-1 module into the existing Cranelift-facing NativeIR.
///
/// Every unsupported representation or control-flow feature is rejected rather
/// than reconstructed through the Rust source frontend.
fn validate_envelope(envelope: &BackendEnvelope) -> Result<(), String> {
    if envelope.abi_major != 1 || envelope.abi_minor > ABI_MINOR {
        return Err(adapter_error(
            "abi",
            format!(
                "expected ABI 1.0 through 1.{ABI_MINOR}, found {}.{}",
                envelope.abi_major, envelope.abi_minor
            ),
        ));
    }
    if envelope.abi_minor < 2 && !envelope.module.structs.is_empty() {
        return Err(adapter_error(
            "layout",
            "struct layouts require managed descriptor adaptation",
        ));
    }
    if envelope.module.name.is_empty() || envelope.module.name.len() > MAX_IDENTITY_BYTES {
        return Err(adapter_error(
            "resource",
            "module identity exceeds its admission bound",
        ));
    }
    if envelope.module.functions.len() > MAX_MODULE_FUNCTIONS {
        return Err(adapter_error(
            "resource",
            format!(
                "module `{}` exceeds the {} function bound",
                envelope.module.name, MAX_MODULE_FUNCTIONS
            ),
        ));
    }
    if envelope.module.functions.is_empty() {
        return Err(adapter_error("module", "native module has no functions"));
    }
    for function in &envelope.module.functions {
        if function.capture_count > function.parameters.len() {
            return Err(adapter_error(
                "closure_captures",
                format!(
                    "function `{}` capture count exceeds its parameter count",
                    function.name
                ),
            ));
        }
        if envelope.abi_minor < 4 && function.capture_count != 0 {
            return Err(adapter_error(
                "closure_captures",
                "capture metadata requires ABI 1.4",
            ));
        }
        if function.name.is_empty() || function.name.len() > MAX_IDENTITY_BYTES {
            return Err(adapter_error(
                "resource",
                format!(
                    "module `{}` contains an invalid function identity",
                    envelope.module.name
                ),
            ));
        }
        if function.blocks.len() > MAX_FUNCTION_BLOCKS {
            return Err(adapter_error(
                "resource",
                format!("function `{}` exceeds the block bound", function.name),
            ));
        }
        let instruction_count = function
            .blocks
            .iter()
            .map(|block| block.instructions.len())
            .try_fold(0usize, usize::checked_add)
            .ok_or_else(|| adapter_error("resource", "instruction count overflow"))?;
        if instruction_count > MAX_FUNCTION_INSTRUCTIONS {
            return Err(adapter_error(
                "resource",
                format!("function `{}` exceeds the instruction bound", function.name),
            ));
        }
        if function.blocks.iter().any(|block| {
            block
                .instructions
                .iter()
                .any(|instruction| instruction.operands.len() > MAX_INSTRUCTION_OPERANDS)
                || block.terminator.operands.len() > MAX_INSTRUCTION_OPERANDS
                || block.terminator.targets.len() > MAX_INSTRUCTION_OPERANDS
        }) {
            return Err(adapter_error(
                "resource",
                format!("function `{}` exceeds the operand bound", function.name),
            ));
        }
    }
    Ok(())
}

fn application_catalog(envelopes: &[BackendEnvelope]) -> Result<FunctionCatalog, String> {
    if envelopes.len() > MAX_APPLICATION_MODULES {
        return Err(adapter_error(
            "resource",
            format!("application exceeds the {MAX_APPLICATION_MODULES} module bound"),
        ));
    }
    let function_count = envelopes
        .iter()
        .map(|envelope| envelope.module.functions.len())
        .try_fold(0usize, usize::checked_add)
        .ok_or_else(|| adapter_error("resource", "application function count overflow"))?;
    if function_count > MAX_APPLICATION_FUNCTIONS {
        return Err(adapter_error(
            "resource",
            format!("application exceeds the {MAX_APPLICATION_FUNCTIONS} function bound"),
        ));
    }
    let mut catalog = FunctionCatalog::new();
    let mut modules = HashSet::new();
    let mut index = 0;
    for envelope in envelopes {
        validate_envelope(envelope)?;
        if !modules.insert(envelope.module.name.clone()) {
            return Err(adapter_error(
                "module",
                format!("application repeats module `{}`", envelope.module.name),
            ));
        }
        for layout in &envelope.module.structs {
            let identity = if layout.name.contains('.') {
                layout.name.clone()
            } else {
                format!("{}.{}", envelope.module.name, layout.name)
            };
            let mut fields = HashMap::new();
            for (index, field) in layout.fields.iter().enumerate() {
                let IrStructField::Typed { name, type_name } = field else {
                    return Err(adapter_error(
                        "struct_layout",
                        format!("struct `{identity}` contains an untyped legacy field"),
                    ));
                };
                if fields
                    .insert(
                        name.clone(),
                        LocalStructField {
                            index,
                            type_name: type_name.clone(),
                        },
                    )
                    .is_some()
                {
                    return Err(adapter_error(
                        "struct_layout",
                        format!("struct `{identity}` repeats field `{name}`"),
                    ));
                }
            }
            if let Some(previous) = catalog.structs.insert(identity.clone(), fields.clone()) {
                if previous != fields {
                    return Err(adapter_error(
                        "struct_layout",
                        format!("application contains conflicting layout `{identity}`"),
                    ));
                }
            }
        }
        for function in &envelope.module.functions {
            let parameters = function
                .parameters
                .iter()
                .map(|parameter| native_type(&parameter.type_name))
                .collect::<Result<Vec<_>, _>>()?;
            let result = native_type(&function.return_type)?;
            let key = (
                envelope.module.name.clone(),
                function.name.clone(),
                function.arity,
            );
            if catalog
                .insert(
                    key,
                    LocalFunction {
                        index,
                        capture_count: function.capture_count,
                        parameters,
                        result,
                    },
                )
                .is_some()
            {
                return Err(adapter_error(
                    "function",
                    format!(
                        "module `{}` repeats function `{}/{}`",
                        envelope.module.name, function.name, function.arity
                    ),
                ));
            }
            index += 1;
        }
    }
    Ok(catalog)
}

fn lower_module(
    envelope: &BackendEnvelope,
    identities: &FunctionCatalog,
) -> Result<NativeModule, String> {
    let mut atoms = Vec::new();
    let functions = envelope
        .module
        .functions
        .iter()
        .map(|function| lower_function(&envelope.module.name, function, identities, &mut atoms))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(NativeModule {
        name: envelope.module.name.clone(),
        functions,
        continuations: Vec::new(),
        managed_layouts: super::self_host_managed_layouts::encoded_struct_layouts(
            std::slice::from_ref(envelope),
        )?,
        managed_collections: super::self_host_managed_layouts::encoded_collection_layouts(
            std::slice::from_ref(envelope),
        )?,
        atoms,
    })
}

fn lower_actor_continuation_body(
    envelope: &BackendEnvelope,
    identities: &FunctionCatalog,
    continuation: &crate::self_host_actor_ir::RuntimeActorContinuation,
    actor_sites: &[super::self_host_actor_adapter::NativeActorSuspensionSite],
    atoms: &mut Vec<String>,
) -> Result<NativeExpr, String> {
    let function = envelope
        .module
        .functions
        .iter()
        .find(|function| {
            function.name == continuation.source_function
                && function.arity == continuation.source_arity
        })
        .ok_or_else(|| {
            adapter_error(
                "actor-continuation-owner",
                format!(
                    "continuation {} references absent function `{}.{}/{}`",
                    continuation.id,
                    envelope.module.name,
                    continuation.source_function,
                    continuation.source_arity
                ),
            )
        })?;
    let sites = actor_sites
        .iter()
        .filter(|site| {
            site.source_function == continuation.source_function
                && site.source_arity == continuation.source_arity
        })
        .cloned()
        .collect::<Vec<_>>();
    validate_actor_sites(function, &sites)?;
    let parameter_types = continuation
        .parameter_types
        .iter()
        .map(|type_name| native_type(type_name))
        .collect::<Result<Vec<_>, _>>()?;
    let values = continuation
        .parameter_names
        .iter()
        .zip(parameter_types.iter().copied())
        .enumerate()
        .map(|(index, (name, value_type))| (name.clone(), (index, value_type)))
        .collect::<ValueCatalog>();
    let return_type = native_type(&continuation.return_type)?;
    let mut expansions = 0;
    lower_block_path(
        &continuation.entry_block,
        None,
        &function.blocks,
        &envelope.module.name,
        values,
        identities,
        &sites,
        return_type,
        atoms,
        &HashSet::new(),
        &mut expansions,
    )?
    .ok_or_else(|| {
        adapter_error(
            "actor-continuation-body",
            format!(
                "continuation {} entry block `{}` has no returning path",
                continuation.id, continuation.entry_block
            ),
        )
    })
}

fn lower_self_hosted_actor_envelope_with_catalog(
    envelope: &BackendEnvelope,
    plan: crate::self_host_actor_ir::RuntimeActorPlan,
    identities: &FunctionCatalog,
) -> Result<NativeModule, String> {
    let mut module = lower_module(envelope, identities)?;
    let actor = super::self_host_actor_adapter::materialize_actor_plan(
        plan,
        native_type,
        |continuation, sites| {
            lower_actor_continuation_body(
                envelope,
                identities,
                continuation,
                sites,
                &mut module.atoms,
            )
        },
    )?;
    module.functions = envelope
        .module
        .functions
        .iter()
        .map(|function| {
            let sites = actor
                .suspension_sites
                .iter()
                .filter(|site| {
                    site.source_function == function.name && site.source_arity == function.arity
                })
                .cloned()
                .collect::<Vec<_>>();
            lower_function_with_actor_sites(
                &envelope.module.name,
                function,
                identities,
                &sites,
                &mut module.atoms,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    module.continuations = actor.continuations;
    Ok(module)
}

fn lower_self_hosted_actor_envelope_with_variants(
    envelope: &BackendEnvelope,
    plan: crate::self_host_actor_ir::RuntimeActorPlan,
    variants: Option<crate::self_host_variant_ir::VariantCatalog>,
) -> Result<NativeModule, String> {
    let mut identities = application_catalog(std::slice::from_ref(envelope))?;
    identities.variants = variants;
    lower_self_hosted_actor_envelope_with_catalog(envelope, plan, &identities)
}

pub(crate) fn lower_self_hosted_actor_envelope(
    envelope: &BackendEnvelope,
    plan: crate::self_host_actor_ir::RuntimeActorPlan,
) -> Result<NativeModule, String> {
    lower_self_hosted_actor_envelope_with_variants(envelope, plan, None)
}

pub(crate) fn lower_self_hosted_typed_actor_envelope(
    envelope: &BackendEnvelope,
    plan: crate::self_host_actor_ir::RuntimeActorPlan,
    variants: crate::self_host_variant_ir::VariantCatalog,
) -> Result<NativeModule, String> {
    lower_self_hosted_actor_envelope_with_variants(envelope, plan, Some(variants))
}

/// Converts an ordered ABI-1 module closure into one globally indexed NativeIR application.
pub(crate) fn lower_self_hosted_application(
    envelopes: &[BackendEnvelope],
) -> Result<Vec<NativeModule>, String> {
    if envelopes.is_empty() {
        return Err(adapter_error(
            "application",
            "application has no ABI modules",
        ));
    }
    let identities = application_catalog(envelopes)?;
    envelopes
        .iter()
        .map(|envelope| lower_module(envelope, &identities))
        .collect()
}

/// Converts a typed actor-aware module closure using one shared variant and call catalog.
pub(crate) fn lower_self_hosted_typed_actor_application(
    envelopes: &[BackendEnvelope],
    plans: Vec<crate::self_host_actor_ir::RuntimeActorPlan>,
    variants: crate::self_host_variant_ir::VariantCatalog,
) -> Result<Vec<NativeModule>, String> {
    if envelopes.is_empty() {
        return Err(adapter_error(
            "typed-actor-application",
            "application has no ABI modules",
        ));
    }
    if envelopes.len() != plans.len() {
        return Err(adapter_error(
            "typed-actor-application",
            format!(
                "application has {} ABI modules but {} actor plans",
                envelopes.len(),
                plans.len()
            ),
        ));
    }
    let mut identities = application_catalog(envelopes)?;
    identities.variants = Some(variants);
    envelopes
        .iter()
        .zip(plans)
        .map(|(envelope, plan)| {
            lower_self_hosted_actor_envelope_with_catalog(envelope, plan, &identities)
        })
        .collect()
}

/// Converts one admitted ABI-1 module into the existing Cranelift-facing NativeIR.
pub(crate) fn lower_self_hosted_envelope(
    envelope: &BackendEnvelope,
) -> Result<NativeModule, String> {
    lower_self_hosted_application(std::slice::from_ref(envelope))
        .map(|mut modules| modules.remove(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::self_host_ir::{IrModule, IrParameter, IrTerminator, ABI_MAJOR, ABI_MINOR};

    fn function(
        module: &str,
        name: &str,
        instructions: Vec<IrInstruction>,
        returned: &str,
    ) -> IrFunction {
        IrFunction {
            module_name: module.to_owned(),
            name: name.to_owned(),
            arity: 1,
            capture_count: 0,
            receiver_mutable: false,
            receiver_command: false,
            parameters: vec![IrParameter {
                name: "value".to_owned(),
                type_name: "Int".to_owned(),
            }],
            return_type: "Int".to_owned(),
            blocks: vec![IrBlock {
                label: "entry".to_owned(),
                instructions,
                terminator: IrTerminator {
                    opcode: "return".to_owned(),
                    operands: vec![returned.to_owned()],
                    targets: Vec::new(),
                },
            }],
            native_symbol: None,
        }
    }

    fn envelope(module: &str, function: IrFunction) -> BackendEnvelope {
        BackendEnvelope {
            abi_major: ABI_MAJOR,
            abi_minor: ABI_MINOR,
            target: "terlan-vm".to_owned(),
            module: IrModule {
                name: module.to_owned(),
                structs: Vec::new(),
                functions: vec![function],
            },
        }
    }

    fn imported_call(module: &str) -> IrInstruction {
        IrInstruction {
            opcode: "call-import".to_owned(),
            result: Some("result".to_owned()),
            operands: vec![module.to_owned(), "id".to_owned(), "value".to_owned()],
            operand_types: Vec::new(),
            type_name: "Int".to_owned(),
            continuation_ordinal: None,
            try_region: None,
        }
    }

    #[test]
    fn imported_call_uses_global_application_index() {
        let provider = envelope("provider", function("provider", "id", Vec::new(), "value"));
        let caller = envelope(
            "caller",
            function("caller", "run", vec![imported_call("provider")], "result"),
        );
        let modules = lower_self_hosted_application(&[provider, caller])
            .expect("admitted cross-module application");
        let NativeExpr::Let { bindings, .. } = &modules[1].functions[0].body else {
            panic!("caller body must bind imported result")
        };
        assert!(matches!(
            bindings.as_slice(),
            [NativeExpr::Call { function: 0, .. }]
        ));
    }

    #[test]
    fn missing_import_target_is_rejected() {
        let caller = envelope(
            "caller",
            function("caller", "run", vec![imported_call("missing")], "result"),
        );
        let error = lower_self_hosted_application(&[caller]).expect_err("missing import");
        assert!(error.contains("not in this ABI module"));
    }
}
