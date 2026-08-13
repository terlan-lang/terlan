//! Backend-independent verification for typed AcceleratorIR kernels.

use std::collections::BTreeMap;

use super::{
    AcceleratorIrAccess, AcceleratorIrBinaryOperation, AcceleratorIrError, AcceleratorIrKernel,
    AcceleratorIrNode, AcceleratorIrOperation, AcceleratorIrType, AcceleratorIrUnaryOperation,
};

/// Verifies one kernel recursively against its declared parameter environment.
pub(super) fn verify_kernel(kernel: &AcceleratorIrKernel) -> Result<(), AcceleratorIrError> {
    let mut locals = kernel
        .parameters
        .iter()
        .map(|parameter| (parameter.name.clone(), parameter.ty.clone()))
        .collect::<BTreeMap<_, _>>();
    verify_node(&kernel.body, &mut locals)
}

/// Verifies one node and all children against exact first-subset types.
fn verify_node(
    node: &AcceleratorIrNode,
    locals: &mut BTreeMap<String, AcceleratorIrType>,
) -> Result<(), AcceleratorIrError> {
    match &node.operation {
        AcceleratorIrOperation::Int { .. } => require_scalar_integer(&node.ty),
        AcceleratorIrOperation::Float { value } => {
            value
                .parse::<f64>()
                .map_err(|_| AcceleratorIrError::TypeMismatch("float literal".to_string()))?;
            require_scalar_float(&node.ty)
        }
        AcceleratorIrOperation::Bool { .. } => require(&node.ty, &AcceleratorIrType::Bool, "bool"),
        AcceleratorIrOperation::Local { name } => {
            let ty = locals
                .get(name)
                .ok_or_else(|| AcceleratorIrError::UnknownLocal(name.clone()))?;
            require(&node.ty, ty, name)
        }
        AcceleratorIrOperation::Let { bindings, body } => {
            let original = locals.clone();
            for (name, value) in bindings {
                verify_node(value, locals)?;
                locals.insert(name.clone(), value.ty.clone());
            }
            verify_node(body, locals)?;
            *locals = original;
            require(&node.ty, &body.ty, "let")
        }
        AcceleratorIrOperation::Unary { operation, operand } => {
            verify_node(operand, locals)?;
            match operation {
                AcceleratorIrUnaryOperation::Negate => require_scalar_number(&operand.ty)?,
                AcceleratorIrUnaryOperation::Not => {
                    require(&operand.ty, &AcceleratorIrType::Bool, "not")?
                }
            }
            require(&node.ty, &operand.ty, "unary")
        }
        AcceleratorIrOperation::Binary {
            operation,
            left,
            right,
        } => {
            verify_node(left, locals)?;
            verify_node(right, locals)?;
            require(&left.ty, &right.ty, "binary operands")?;
            match operation {
                AcceleratorIrBinaryOperation::And | AcceleratorIrBinaryOperation::Or => {
                    require(&left.ty, &AcceleratorIrType::Bool, "Boolean operation")?
                }
                _ => require_scalar_number(&left.ty)?,
            }
            require(&node.ty, &left.ty, "binary result")
        }
        AcceleratorIrOperation::Compare { left, right, .. } => {
            verify_node(left, locals)?;
            verify_node(right, locals)?;
            require(&left.ty, &right.ty, "comparison operands")?;
            require_scalar_or_bool(&left.ty)?;
            require(&node.ty, &AcceleratorIrType::Bool, "comparison result")
        }
        AcceleratorIrOperation::If {
            condition,
            then_value,
            else_value,
        } => {
            verify_node(condition, locals)?;
            verify_node(then_value, locals)?;
            verify_node(else_value, locals)?;
            require(&condition.ty, &AcceleratorIrType::Bool, "if condition")?;
            require(&then_value.ty, &else_value.ty, "if branches")?;
            require(&node.ty, &then_value.ty, "if result")
        }
        AcceleratorIrOperation::Load { buffer, index } => {
            verify_node(index, locals)?;
            require_scalar_integer(&index.ty)?;
            let (dtype, access) = buffer_contract(locals, buffer)?;
            if access == AcceleratorIrAccess::Write {
                return Err(AcceleratorIrError::UnsupportedEffect(format!(
                    "read from write-only buffer `{buffer}`"
                )));
            }
            require(
                &node.ty,
                &AcceleratorIrType::Scalar { dtype },
                "buffer load",
            )
        }
        AcceleratorIrOperation::Store {
            buffer,
            index,
            value,
        } => {
            verify_node(index, locals)?;
            verify_node(value, locals)?;
            require_scalar_integer(&index.ty)?;
            let (dtype, access) = buffer_contract(locals, buffer)?;
            if access == AcceleratorIrAccess::Read {
                return Err(AcceleratorIrError::UnsupportedEffect(format!(
                    "write to read-only buffer `{buffer}`"
                )));
            }
            require(
                &value.ty,
                &AcceleratorIrType::Scalar { dtype },
                "buffer store value",
            )?;
            require(&node.ty, &AcceleratorIrType::Unit, "buffer store result")
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
            if !(0..=1_000_000).contains(&iterations) || index_name == accumulator_name {
                return Err(AcceleratorIrError::InvalidStaticLoop);
            }
            verify_node(initial, locals)?;
            let original = locals.clone();
            locals.insert(index_name.clone(), scalar_i64());
            locals.insert(accumulator_name.clone(), initial.ty.clone());
            verify_node(body, locals)?;
            *locals = original;
            require(&body.ty, &initial.ty, "loop accumulator")?;
            require(&node.ty, &initial.ty, "loop result")
        }
        AcceleratorIrOperation::Math {
            operation,
            arguments,
        } => {
            if arguments.is_empty() {
                return Err(AcceleratorIrError::MissingPackageOperation(
                    operation.clone(),
                ));
            }
            for argument in arguments {
                verify_node(argument, locals)?;
                require_scalar_float(&argument.ty)?;
                require(&argument.ty, &node.ty, operation)?;
            }
            Ok(())
        }
    }
}

/// Returns the scalar dtype and access contract of one buffer local.
fn buffer_contract(
    locals: &BTreeMap<String, AcceleratorIrType>,
    name: &str,
) -> Result<(super::AcceleratorScalarType, AcceleratorIrAccess), AcceleratorIrError> {
    match locals.get(name) {
        Some(AcceleratorIrType::Buffer { dtype, access, .. }) => Ok((*dtype, *access)),
        _ => Err(AcceleratorIrError::UnsupportedType(name.to_string())),
    }
}

/// Requires exact type equality with context.
fn require(
    actual: &AcceleratorIrType,
    expected: &AcceleratorIrType,
    context: &str,
) -> Result<(), AcceleratorIrError> {
    if actual == expected {
        Ok(())
    } else {
        Err(AcceleratorIrError::TypeMismatch(context.to_string()))
    }
}

/// Requires a scalar integer type.
fn require_scalar_integer(ty: &AcceleratorIrType) -> Result<(), AcceleratorIrError> {
    match ty {
        AcceleratorIrType::Scalar { dtype } if dtype.is_integer() => Ok(()),
        _ => Err(AcceleratorIrError::TypeMismatch(
            "integer scalar".to_string(),
        )),
    }
}

/// Requires a scalar floating-point type.
fn require_scalar_float(ty: &AcceleratorIrType) -> Result<(), AcceleratorIrError> {
    match ty {
        AcceleratorIrType::Scalar { dtype } if dtype.is_float() => Ok(()),
        _ => Err(AcceleratorIrError::TypeMismatch(
            "floating scalar".to_string(),
        )),
    }
}

/// Requires any scalar number type.
fn require_scalar_number(ty: &AcceleratorIrType) -> Result<(), AcceleratorIrError> {
    match ty {
        AcceleratorIrType::Scalar { .. } => Ok(()),
        _ => Err(AcceleratorIrError::TypeMismatch(
            "numeric scalar".to_string(),
        )),
    }
}

/// Requires a scalar number or Boolean predicate.
fn require_scalar_or_bool(ty: &AcceleratorIrType) -> Result<(), AcceleratorIrError> {
    if matches!(
        ty,
        AcceleratorIrType::Scalar { .. } | AcceleratorIrType::Bool
    ) {
        Ok(())
    } else {
        Err(AcceleratorIrError::TypeMismatch(
            "comparable scalar".to_string(),
        ))
    }
}

/// Returns the canonical signed index type.
fn scalar_i64() -> AcceleratorIrType {
    AcceleratorIrType::Scalar {
        dtype: super::AcceleratorScalarType::I64,
    }
}
