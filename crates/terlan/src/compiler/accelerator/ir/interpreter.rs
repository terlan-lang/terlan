//! CPU reference interpreter for the first pure AcceleratorIR subset.

use std::collections::BTreeMap;

use super::{
    AcceleratorIrBinaryOperation, AcceleratorIrComparison, AcceleratorIrError, AcceleratorIrKernel,
    AcceleratorIrNode, AcceleratorIrOperation, AcceleratorIrUnaryOperation,
};

/// Runtime value accepted by the AcceleratorIR reference interpreter.
#[derive(Clone, Debug, PartialEq)]
pub enum AcceleratorIrValue {
    /// Signed integer scalar.
    Int(i64),
    /// IEEE floating-point scalar.
    Float(f64),
    /// Boolean predicate.
    Bool(bool),
    /// Unit result.
    Unit,
    /// Mutable typed buffer represented by scalar interpreter values.
    Buffer(Vec<AcceleratorIrValue>),
}

/// Deterministic CPU reference interpreter for differential validation.
#[derive(Debug, Default)]
pub struct AcceleratorIrInterpreter;

impl AcceleratorIrInterpreter {
    /// Executes one kernel body with exact named arguments.
    pub fn execute(
        kernel: &AcceleratorIrKernel,
        arguments: BTreeMap<String, AcceleratorIrValue>,
    ) -> Result<AcceleratorIrValue, AcceleratorIrError> {
        if arguments.len() != kernel.parameters.len()
            || kernel
                .parameters
                .iter()
                .any(|parameter| !arguments.contains_key(&parameter.name))
        {
            return Err(AcceleratorIrError::UnknownLocal(
                "kernel arguments".to_string(),
            ));
        }
        let mut environment = arguments;
        evaluate(&kernel.body, &mut environment)
    }
}

/// Recursively evaluates one typed node.
fn evaluate(
    node: &AcceleratorIrNode,
    environment: &mut BTreeMap<String, AcceleratorIrValue>,
) -> Result<AcceleratorIrValue, AcceleratorIrError> {
    match &node.operation {
        AcceleratorIrOperation::Int { value } => Ok(AcceleratorIrValue::Int(*value)),
        AcceleratorIrOperation::Float { value } => value
            .parse::<f64>()
            .map(AcceleratorIrValue::Float)
            .map_err(|_| AcceleratorIrError::TypeMismatch("float literal".to_string())),
        AcceleratorIrOperation::Bool { value } => Ok(AcceleratorIrValue::Bool(*value)),
        AcceleratorIrOperation::Local { name } => environment
            .get(name)
            .cloned()
            .ok_or_else(|| AcceleratorIrError::UnknownLocal(name.clone())),
        AcceleratorIrOperation::Let { bindings, body } => {
            let mut previous = Vec::new();
            for (name, value) in bindings {
                let value = evaluate(value, environment)?;
                previous.push((name.clone(), environment.insert(name.clone(), value)));
            }
            let result = evaluate(body, environment);
            for (name, value) in previous.into_iter().rev() {
                match value {
                    Some(value) => {
                        environment.insert(name, value);
                    }
                    None => {
                        environment.remove(&name);
                    }
                }
            }
            result
        }
        AcceleratorIrOperation::Unary { operation, operand } => {
            evaluate(operand, environment).and_then(|value| unary(*operation, value))
        }
        AcceleratorIrOperation::Binary {
            operation,
            left,
            right,
        } => {
            let left = evaluate(left, environment)?;
            let right = evaluate(right, environment)?;
            binary(*operation, left, right)
        }
        AcceleratorIrOperation::Compare {
            comparison,
            left,
            right,
        } => {
            let left = evaluate(left, environment)?;
            let right = evaluate(right, environment)?;
            compare(*comparison, left, right).map(AcceleratorIrValue::Bool)
        }
        AcceleratorIrOperation::If {
            condition,
            then_value,
            else_value,
        } => match evaluate(condition, environment)? {
            AcceleratorIrValue::Bool(true) => evaluate(then_value, environment),
            AcceleratorIrValue::Bool(false) => evaluate(else_value, environment),
            _ => Err(AcceleratorIrError::TypeMismatch("if condition".to_string())),
        },
        AcceleratorIrOperation::Load { buffer, index } => {
            let index = index_value(evaluate(index, environment)?)?;
            let Some(AcceleratorIrValue::Buffer(values)) = environment.get(buffer) else {
                return Err(AcceleratorIrError::UnknownLocal(buffer.clone()));
            };
            values.get(index).cloned().ok_or_else(|| {
                AcceleratorIrError::InvalidMemoryContract(format!(
                    "buffer `{buffer}` index {index} is out of bounds"
                ))
            })
        }
        AcceleratorIrOperation::Store {
            buffer,
            index,
            value,
        } => {
            let index = index_value(evaluate(index, environment)?)?;
            let value = evaluate(value, environment)?;
            let Some(AcceleratorIrValue::Buffer(values)) = environment.get_mut(buffer) else {
                return Err(AcceleratorIrError::UnknownLocal(buffer.clone()));
            };
            let slot = values.get_mut(index).ok_or_else(|| {
                AcceleratorIrError::InvalidMemoryContract(format!(
                    "buffer `{buffer}` index {index} is out of bounds"
                ))
            })?;
            *slot = value;
            Ok(AcceleratorIrValue::Unit)
        }
        AcceleratorIrOperation::StaticLoop {
            index_name,
            start,
            end,
            accumulator_name,
            initial,
            body,
        } => {
            let iterations = end
                .checked_sub(*start)
                .ok_or(AcceleratorIrError::InvalidStaticLoop)?;
            if !(0..=1_000_000).contains(&iterations) {
                return Err(AcceleratorIrError::InvalidStaticLoop);
            }
            let old_index = environment.get(index_name).cloned();
            let old_accumulator = environment.get(accumulator_name).cloned();
            let mut accumulator = evaluate(initial, environment)?;
            for index in *start..*end {
                environment.insert(index_name.clone(), AcceleratorIrValue::Int(index));
                environment.insert(accumulator_name.clone(), accumulator);
                accumulator = evaluate(body, environment)?;
            }
            restore(environment, index_name, old_index);
            restore(environment, accumulator_name, old_accumulator);
            Ok(accumulator)
        }
        AcceleratorIrOperation::Math {
            operation,
            arguments,
        } => {
            let arguments = arguments
                .iter()
                .map(|argument| evaluate(argument, environment))
                .collect::<Result<Vec<_>, _>>()?;
            math(operation, &arguments)
        }
    }
}

/// Restores one shadowed interpreter local.
fn restore(
    environment: &mut BTreeMap<String, AcceleratorIrValue>,
    name: &str,
    value: Option<AcceleratorIrValue>,
) {
    match value {
        Some(value) => {
            environment.insert(name.to_string(), value);
        }
        None => {
            environment.remove(name);
        }
    }
}

/// Evaluates one unary operation.
fn unary(
    operation: AcceleratorIrUnaryOperation,
    value: AcceleratorIrValue,
) -> Result<AcceleratorIrValue, AcceleratorIrError> {
    match (operation, value) {
        (AcceleratorIrUnaryOperation::Negate, AcceleratorIrValue::Int(value)) => value
            .checked_neg()
            .map(AcceleratorIrValue::Int)
            .ok_or_else(|| AcceleratorIrError::TypeMismatch("integer negation".to_string())),
        (AcceleratorIrUnaryOperation::Negate, AcceleratorIrValue::Float(value)) => {
            Ok(AcceleratorIrValue::Float(-value))
        }
        (AcceleratorIrUnaryOperation::Not, AcceleratorIrValue::Bool(value)) => {
            Ok(AcceleratorIrValue::Bool(!value))
        }
        _ => Err(AcceleratorIrError::TypeMismatch(
            "unary operation".to_string(),
        )),
    }
}

/// Evaluates one checked binary operation.
fn binary(
    operation: AcceleratorIrBinaryOperation,
    left: AcceleratorIrValue,
    right: AcceleratorIrValue,
) -> Result<AcceleratorIrValue, AcceleratorIrError> {
    use AcceleratorIrBinaryOperation as Operation;
    match (operation, left, right) {
        (Operation::Add, AcceleratorIrValue::Int(left), AcceleratorIrValue::Int(right)) => left
            .checked_add(right)
            .map(AcceleratorIrValue::Int)
            .ok_or_else(|| AcceleratorIrError::TypeMismatch("integer addition".to_string())),
        (Operation::Subtract, AcceleratorIrValue::Int(left), AcceleratorIrValue::Int(right)) => {
            left.checked_sub(right)
                .map(AcceleratorIrValue::Int)
                .ok_or_else(|| AcceleratorIrError::TypeMismatch("integer subtraction".to_string()))
        }
        (Operation::Multiply, AcceleratorIrValue::Int(left), AcceleratorIrValue::Int(right)) => {
            left.checked_mul(right)
                .map(AcceleratorIrValue::Int)
                .ok_or_else(|| {
                    AcceleratorIrError::TypeMismatch("integer multiplication".to_string())
                })
        }
        (Operation::Divide, AcceleratorIrValue::Int(_), AcceleratorIrValue::Int(0))
        | (Operation::Remainder, AcceleratorIrValue::Int(_), AcceleratorIrValue::Int(0)) => Err(
            AcceleratorIrError::TypeMismatch("integer division by zero".to_string()),
        ),
        (Operation::Divide, AcceleratorIrValue::Int(left), AcceleratorIrValue::Int(right)) => left
            .checked_div(right)
            .map(AcceleratorIrValue::Int)
            .ok_or_else(|| AcceleratorIrError::TypeMismatch("integer division".to_string())),
        (Operation::Remainder, AcceleratorIrValue::Int(left), AcceleratorIrValue::Int(right)) => {
            left.checked_rem(right)
                .map(AcceleratorIrValue::Int)
                .ok_or_else(|| AcceleratorIrError::TypeMismatch("integer remainder".to_string()))
        }
        (operation, AcceleratorIrValue::Float(left), AcceleratorIrValue::Float(right)) => {
            let value = match operation {
                Operation::Add => left + right,
                Operation::Subtract => left - right,
                Operation::Multiply => left * right,
                Operation::Divide => left / right,
                Operation::Remainder => left % right,
                Operation::And | Operation::Or => {
                    return Err(AcceleratorIrError::TypeMismatch(
                        "float Boolean operation".to_string(),
                    ))
                }
            };
            Ok(AcceleratorIrValue::Float(value))
        }
        (Operation::And, AcceleratorIrValue::Bool(left), AcceleratorIrValue::Bool(right)) => {
            Ok(AcceleratorIrValue::Bool(left && right))
        }
        (Operation::Or, AcceleratorIrValue::Bool(left), AcceleratorIrValue::Bool(right)) => {
            Ok(AcceleratorIrValue::Bool(left || right))
        }
        _ => Err(AcceleratorIrError::TypeMismatch(
            "binary operation".to_string(),
        )),
    }
}

/// Evaluates one comparison over matching scalar variants.
fn compare(
    comparison: AcceleratorIrComparison,
    left: AcceleratorIrValue,
    right: AcceleratorIrValue,
) -> Result<bool, AcceleratorIrError> {
    let ordering = match (&left, &right) {
        (AcceleratorIrValue::Int(left), AcceleratorIrValue::Int(right)) => left.partial_cmp(right),
        (AcceleratorIrValue::Float(left), AcceleratorIrValue::Float(right)) => {
            left.partial_cmp(right)
        }
        (AcceleratorIrValue::Bool(left), AcceleratorIrValue::Bool(right)) => {
            left.partial_cmp(right)
        }
        _ => return Err(AcceleratorIrError::TypeMismatch("comparison".to_string())),
    };
    Ok(match comparison {
        AcceleratorIrComparison::Equal => left == right,
        AcceleratorIrComparison::NotEqual => left != right,
        AcceleratorIrComparison::Less => ordering.is_some_and(|value| value.is_lt()),
        AcceleratorIrComparison::LessEqual => ordering.is_some_and(|value| value.is_le()),
        AcceleratorIrComparison::Greater => ordering.is_some_and(|value| value.is_gt()),
        AcceleratorIrComparison::GreaterEqual => ordering.is_some_and(|value| value.is_ge()),
    })
}

/// Evaluates a maintained package-declared scalar math operation by semantic identity.
fn math(
    operation: &str,
    arguments: &[AcceleratorIrValue],
) -> Result<AcceleratorIrValue, AcceleratorIrError> {
    let floats = arguments
        .iter()
        .map(|value| match value {
            AcceleratorIrValue::Float(value) => Ok(*value),
            _ => Err(AcceleratorIrError::TypeMismatch(operation.to_string())),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let value = match (operation.rsplit('.').next(), floats.as_slice()) {
        (Some("sqrt"), [value]) => value.sqrt(),
        (Some("abs"), [value]) => value.abs(),
        (Some("min"), [left, right]) => left.min(*right),
        (Some("max"), [left, right]) => left.max(*right),
        _ => {
            return Err(AcceleratorIrError::MissingPackageOperation(
                operation.to_string(),
            ))
        }
    };
    Ok(AcceleratorIrValue::Float(value))
}

/// Converts a nonnegative signed scalar to a host index.
fn index_value(value: AcceleratorIrValue) -> Result<usize, AcceleratorIrError> {
    match value {
        AcceleratorIrValue::Int(value) => usize::try_from(value)
            .map_err(|_| AcceleratorIrError::InvalidMemoryContract("negative index".to_string())),
        _ => Err(AcceleratorIrError::TypeMismatch("buffer index".to_string())),
    }
}
