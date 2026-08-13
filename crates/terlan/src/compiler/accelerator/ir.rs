//! Typed backend-neutral accelerator IR lowered only from checked CoreIR.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::compiler::typeck::{
    CoreExpr, CoreExprSummary, CoreFunction, CoreModule, CorePattern, CoreType,
};

use super::AcceleratorScalarType;

#[path = "ir/interpreter.rs"]
mod interpreter;
pub use interpreter::{AcceleratorIrInterpreter, AcceleratorIrValue};

#[path = "ir/verify.rs"]
mod verify;

/// Stable serialized AcceleratorIR schema.
pub const ACCELERATOR_IR_SCHEMA: &str = "terlan.accelerator-ir.v1";

/// Maximum dynamic shared memory admitted by the backend-neutral verifier.
pub const ACCELERATOR_IR_MAX_SHARED_MEMORY_BYTES: u64 = 1 << 30;

/// Source location preserved from the compiler-selected kernel declaration.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorIrSource {
    /// Source file identity.
    pub file: String,
    /// One-based source line.
    pub line: u32,
    /// One-based source column.
    pub column: u32,
}

/// Explicit kernel execution dimensions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorExecutionDimensions {
    /// Work-group count in X, Y, and Z.
    pub grid: [u32; 3],
    /// Threads or lanes per work group in X, Y, and Z.
    pub block: [u32; 3],
}

impl AcceleratorExecutionDimensions {
    /// Validates nonzero dimensions and checked total lane count.
    pub fn validate(self) -> Result<(), AcceleratorIrError> {
        if self.grid.contains(&0) || self.block.contains(&0) {
            return Err(AcceleratorIrError::InvalidExecutionDimensions);
        }
        self.grid
            .into_iter()
            .chain(self.block)
            .try_fold(1u64, |total, value| total.checked_mul(u64::from(value)))
            .ok_or(AcceleratorIrError::InvalidExecutionDimensions)?;
        Ok(())
    }
}

/// Accelerator address space represented without backend pointers.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcceleratorIrAddressSpace {
    /// Kernel-local scalar or bounded fixed storage.
    Local,
    /// Work-group shared memory.
    Shared,
    /// Device-global storage supplied by a kernel parameter.
    Device,
    /// Read-only constant storage.
    Constant,
}

/// Buffer access contract used by alias and mutation verification.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcceleratorIrAccess {
    /// Buffer can only be read.
    Read,
    /// Buffer can only be written.
    Write,
    /// Buffer can be read and written.
    ReadWrite,
}

/// Closed AcceleratorIR value types.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AcceleratorIrType {
    /// Scalar value represented by the canonical accelerator dtype model.
    Scalar { dtype: AcceleratorScalarType },
    /// Boolean predicate.
    Bool,
    /// Unit result used by stores and synchronization.
    Unit,
    /// Typed buffer with explicit memory and alias contract.
    Buffer {
        /// Scalar element type.
        dtype: AcceleratorScalarType,
        /// Logical address space.
        address_space: AcceleratorIrAddressSpace,
        /// Mutation contract.
        access: AcceleratorIrAccess,
        /// Required byte alignment.
        alignment: u64,
        /// Nonzero alias class; equal classes may alias.
        alias_class: u32,
    },
}

/// One checked kernel parameter.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorIrParameter {
    /// Parameter identity inherited from CoreIR.
    pub name: String,
    /// Closed AcceleratorIR parameter type.
    pub ty: AcceleratorIrType,
}

/// Unary scalar operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcceleratorIrUnaryOperation {
    /// Arithmetic negation.
    Negate,
    /// Boolean negation.
    Not,
}

/// Binary scalar arithmetic operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcceleratorIrBinaryOperation {
    /// Addition.
    Add,
    /// Subtraction.
    Subtract,
    /// Multiplication.
    Multiply,
    /// Division.
    Divide,
    /// Remainder.
    Remainder,
    /// Boolean conjunction.
    And,
    /// Boolean disjunction.
    Or,
}

/// Scalar comparison operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcceleratorIrComparison {
    /// Equality.
    Equal,
    /// Inequality.
    NotEqual,
    /// Less-than.
    Less,
    /// Less-than-or-equal.
    LessEqual,
    /// Greater-than.
    Greater,
    /// Greater-than-or-equal.
    GreaterEqual,
}

/// Typed AcceleratorIR expression node.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorIrNode {
    /// Checked result type.
    pub ty: AcceleratorIrType,
    /// Source location retained for diagnostics and backend lowering.
    pub source: AcceleratorIrSource,
    /// Structured operation.
    pub operation: AcceleratorIrOperation,
}

/// Structured first-subset AcceleratorIR operations.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AcceleratorIrOperation {
    /// Signed integer scalar literal.
    Int { value: i64 },
    /// IEEE floating-point literal retained in deterministic text form.
    Float { value: String },
    /// Boolean scalar literal.
    Bool { value: bool },
    /// Local or parameter reference.
    Local { name: String },
    /// Ordered local bindings followed by one result expression.
    Let {
        /// Name and value pairs.
        bindings: Vec<(String, AcceleratorIrNode)>,
        /// Result expression.
        body: Box<AcceleratorIrNode>,
    },
    /// Unary scalar operation.
    Unary {
        /// Operation identity.
        operation: AcceleratorIrUnaryOperation,
        /// Operand.
        operand: Box<AcceleratorIrNode>,
    },
    /// Binary scalar operation.
    Binary {
        /// Operation identity.
        operation: AcceleratorIrBinaryOperation,
        /// Left operand.
        left: Box<AcceleratorIrNode>,
        /// Right operand.
        right: Box<AcceleratorIrNode>,
    },
    /// Scalar comparison.
    Compare {
        /// Comparison identity.
        comparison: AcceleratorIrComparison,
        /// Left operand.
        left: Box<AcceleratorIrNode>,
        /// Right operand.
        right: Box<AcceleratorIrNode>,
    },
    /// Structured branch with equal result types.
    If {
        /// Boolean condition.
        condition: Box<AcceleratorIrNode>,
        /// Value produced when true.
        then_value: Box<AcceleratorIrNode>,
        /// Value produced when false.
        else_value: Box<AcceleratorIrNode>,
    },
    /// Bounds-checked typed buffer load.
    Load {
        /// Buffer parameter or local identity.
        buffer: String,
        /// Element index.
        index: Box<AcceleratorIrNode>,
    },
    /// Bounds-checked typed buffer store.
    Store {
        /// Buffer parameter or local identity.
        buffer: String,
        /// Element index.
        index: Box<AcceleratorIrNode>,
        /// Stored scalar value.
        value: Box<AcceleratorIrNode>,
    },
    /// Compile-time bounded loop carrying one accumulator.
    StaticLoop {
        /// Loop index local.
        index_name: String,
        /// Inclusive start value.
        start: i64,
        /// Exclusive end value.
        end: i64,
        /// Accumulator local.
        accumulator_name: String,
        /// Initial accumulator value.
        initial: Box<AcceleratorIrNode>,
        /// Body producing the next accumulator value.
        body: Box<AcceleratorIrNode>,
    },
    /// Package-declared pure math operation.
    Math {
        /// Fully qualified package operation.
        operation: String,
        /// Ordered scalar arguments.
        arguments: Vec<AcceleratorIrNode>,
    },
}

/// Complete typed kernel lowered from one checked CoreIR function.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorIrKernel {
    /// Stable kernel identity.
    pub name: String,
    /// Source CoreIR module and function identity.
    pub core_identity: String,
    /// Concrete generic substitutions selected before lowering.
    pub specializations: BTreeMap<String, AcceleratorIrType>,
    /// Ordered kernel parameters.
    pub parameters: Vec<AcceleratorIrParameter>,
    /// Checked return type.
    pub return_type: AcceleratorIrType,
    /// Explicit launch dimensions.
    pub dimensions: AcceleratorExecutionDimensions,
    /// Maximum dynamic shared memory required by the kernel.
    pub shared_memory_bytes: u64,
    /// Named synchronization points retained for backend verification.
    pub synchronization_points: Vec<String>,
    /// Typed executable body.
    pub body: AcceleratorIrNode,
    /// Kernel declaration source.
    pub source: AcceleratorIrSource,
}

/// Serializable module containing backend-neutral accelerator kernels.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceleratorIrModule {
    /// Stable schema identity.
    pub schema: &'static str,
    /// Source CoreIR module.
    pub module: String,
    /// Deterministically ordered kernels.
    pub kernels: Vec<AcceleratorIrKernel>,
}

/// Compiler-selected lowering contract for one kernel function.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceleratorKernelSelection {
    /// CoreIR function name.
    pub function: String,
    /// Concrete generic substitutions.
    pub specializations: BTreeMap<String, AcceleratorIrType>,
    /// Explicit buffer parameter contracts by parameter name.
    pub buffer_parameters: BTreeMap<String, AcceleratorIrType>,
    /// Explicit launch dimensions.
    pub dimensions: AcceleratorExecutionDimensions,
    /// Dynamic shared-memory requirement.
    pub shared_memory_bytes: u64,
    /// Named synchronization points.
    pub synchronization_points: Vec<String>,
    /// Package-declared pure math operations admitted inside the kernel.
    pub math_operations: BTreeSet<String>,
    /// Source location selected from the checked declaration.
    pub source: AcceleratorIrSource,
}

/// Typed AcceleratorIR rejection before backend invocation.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "kebab-case")]
pub enum AcceleratorIrError {
    /// Selected function is absent or ambiguous.
    MissingFunction(String),
    /// Function has no single typed executable body.
    MissingTypedBody(String),
    /// Source CoreIR type is outside the kernel subset.
    UnsupportedType(String),
    /// Source CoreIR operation is outside the kernel subset.
    UnsupportedOperation(String),
    /// Local or parameter identity is unresolved.
    UnknownLocal(String),
    /// Operand or branch types disagree.
    TypeMismatch(String),
    /// Dynamic or recursive call is forbidden.
    DynamicCall(String),
    /// Actor, I/O, allocation, exception, closure, or runtime effect is forbidden.
    UnsupportedEffect(String),
    /// Package operation lacks an accelerator implementation declaration.
    MissingPackageOperation(String),
    /// Static loop bound is invalid or exceeds the compiler limit.
    InvalidStaticLoop,
    /// Execution dimensions are zero or overflow.
    InvalidExecutionDimensions,
    /// Shared-memory or alignment contract is invalid.
    InvalidMemoryContract(String),
}

impl AcceleratorIrError {
    /// Returns the stable diagnostic code for this rejection class.
    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingFunction(_) => "accelerator.ir-missing-function",
            Self::MissingTypedBody(_) => "accelerator.ir-missing-body",
            Self::UnsupportedType(_) => "accelerator.ir-type",
            Self::UnsupportedOperation(_) => "accelerator.ir-operation",
            Self::UnknownLocal(_) => "accelerator.ir-local",
            Self::TypeMismatch(_) => "accelerator.ir-type-mismatch",
            Self::DynamicCall(_) => "accelerator.ir-dynamic-call",
            Self::UnsupportedEffect(_) => "accelerator.ir-effect",
            Self::MissingPackageOperation(_) => "accelerator.ir-package-operation",
            Self::InvalidStaticLoop => "accelerator.ir-static-loop",
            Self::InvalidExecutionDimensions => "accelerator.ir-dimensions",
            Self::InvalidMemoryContract(_) => "accelerator.ir-memory",
        }
    }
}

impl std::fmt::Display for AcceleratorIrError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "error[{}]: {self:?}", self.code())
    }
}

impl std::error::Error for AcceleratorIrError {}

impl AcceleratorIrModule {
    /// Lowers selected functions from checked CoreIR into AcceleratorIR.
    pub fn lower(
        core: &CoreModule,
        selections: &[AcceleratorKernelSelection],
    ) -> Result<Self, AcceleratorIrError> {
        let mut kernels = selections
            .iter()
            .map(|selection| lower_kernel(core, selection))
            .collect::<Result<Vec<_>, _>>()?;
        kernels.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(Self {
            schema: ACCELERATOR_IR_SCHEMA,
            module: core.module.clone(),
            kernels,
        })
    }

    /// Returns a deterministic SHA-256 identity over canonical JSON.
    pub fn normalized_hash(&self) -> Result<String, AcceleratorIrError> {
        let bytes = serde_json::to_vec(self)
            .map_err(|error| AcceleratorIrError::UnsupportedOperation(error.to_string()))?;
        Ok(Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }

    /// Verifies all explicit contracts independently of backend lowering.
    pub fn verify(&self) -> Result<(), AcceleratorIrError> {
        if self.schema != ACCELERATOR_IR_SCHEMA {
            return Err(AcceleratorIrError::UnsupportedOperation(
                "schema".to_string(),
            ));
        }
        for kernel in &self.kernels {
            kernel.dimensions.validate()?;
            if kernel.shared_memory_bytes > ACCELERATOR_IR_MAX_SHARED_MEMORY_BYTES {
                return Err(AcceleratorIrError::InvalidMemoryContract(format!(
                    "kernel `{}` requests {} shared-memory bytes",
                    kernel.name, kernel.shared_memory_bytes
                )));
            }
            let mut parameters = BTreeSet::new();
            for parameter in &kernel.parameters {
                if !parameters.insert(&parameter.name) {
                    return Err(AcceleratorIrError::InvalidMemoryContract(format!(
                        "duplicate parameter `{}`",
                        parameter.name
                    )));
                }
                if let AcceleratorIrType::Buffer {
                    dtype,
                    alignment,
                    alias_class,
                    ..
                } = parameter.ty
                {
                    if alignment < dtype.alignment()
                        || !alignment.is_power_of_two()
                        || alias_class == 0
                    {
                        return Err(AcceleratorIrError::InvalidMemoryContract(
                            parameter.name.clone(),
                        ));
                    }
                }
            }
            if kernel.body.ty != kernel.return_type {
                return Err(AcceleratorIrError::TypeMismatch(kernel.name.clone()));
            }
            verify::verify_kernel(kernel)?;
        }
        Ok(())
    }
}

/// Lowers one selected CoreIR function.
fn lower_kernel(
    core: &CoreModule,
    selection: &AcceleratorKernelSelection,
) -> Result<AcceleratorIrKernel, AcceleratorIrError> {
    selection.dimensions.validate()?;
    let functions = core
        .functions
        .iter()
        .filter(|function| function.name == selection.function)
        .collect::<Vec<_>>();
    let [function] = functions.as_slice() else {
        return Err(AcceleratorIrError::MissingFunction(
            selection.function.clone(),
        ));
    };
    let [clause] = function.clauses.as_slice() else {
        return Err(AcceleratorIrError::MissingTypedBody(
            selection.function.clone(),
        ));
    };
    if clause.guard.is_some() {
        return Err(AcceleratorIrError::UnsupportedOperation(
            "function guard".to_string(),
        ));
    }
    let mut locals = BTreeMap::new();
    let parameters = lower_parameters(function, selection, &mut locals)?;
    let return_type = lower_core_type(
        function
            .core_return_type
            .as_ref()
            .ok_or_else(|| AcceleratorIrError::UnsupportedType(function.return_type.clone()))?,
    )?;
    let body = lower_summary(&clause.body, selection, &mut locals)?;
    if body.ty != return_type {
        return Err(AcceleratorIrError::TypeMismatch(format!(
            "{}/{} return",
            function.name, function.arity
        )));
    }
    Ok(AcceleratorIrKernel {
        name: function.name.clone(),
        core_identity: format!("{}.{}/{}", core.module, function.name, function.arity),
        specializations: selection.specializations.clone(),
        parameters,
        return_type,
        dimensions: selection.dimensions,
        shared_memory_bytes: selection.shared_memory_bytes,
        synchronization_points: selection.synchronization_points.clone(),
        body,
        source: selection.source.clone(),
    })
}

/// Lowers function parameters and explicit buffer metadata.
fn lower_parameters(
    function: &CoreFunction,
    selection: &AcceleratorKernelSelection,
    locals: &mut BTreeMap<String, AcceleratorIrType>,
) -> Result<Vec<AcceleratorIrParameter>, AcceleratorIrError> {
    function
        .params
        .iter()
        .map(|parameter| {
            let ty = selection
                .buffer_parameters
                .get(&parameter.name)
                .cloned()
                .or_else(|| {
                    parameter
                        .core_ty
                        .as_ref()
                        .and_then(|ty| lower_core_type(ty).ok())
                })
                .ok_or_else(|| AcceleratorIrError::UnsupportedType(parameter.ty.clone()))?;
            locals.insert(parameter.name.clone(), ty.clone());
            Ok(AcceleratorIrParameter {
                name: parameter.name.clone(),
                ty,
            })
        })
        .collect()
}

/// Lowers one CoreIR expression summary only when its typed payload exists.
fn lower_summary(
    summary: &CoreExprSummary,
    selection: &AcceleratorKernelSelection,
    locals: &mut BTreeMap<String, AcceleratorIrType>,
) -> Result<AcceleratorIrNode, AcceleratorIrError> {
    let expr = summary
        .core_expr
        .as_ref()
        .ok_or_else(|| AcceleratorIrError::MissingTypedBody(summary.kind.clone()))?;
    lower_expr(expr, selection, locals)
}

/// Lowers one typed CoreIR expression into the bounded kernel subset.
fn lower_expr(
    expr: &CoreExpr,
    selection: &AcceleratorKernelSelection,
    locals: &mut BTreeMap<String, AcceleratorIrType>,
) -> Result<AcceleratorIrNode, AcceleratorIrError> {
    let source = selection.source.clone();
    let node = |ty, operation| AcceleratorIrNode {
        ty,
        source: source.clone(),
        operation,
    };
    match expr {
        CoreExpr::Int(value) => Ok(node(
            scalar(AcceleratorScalarType::I64),
            AcceleratorIrOperation::Int { value: *value },
        )),
        CoreExpr::Float(value) => Ok(node(
            scalar(AcceleratorScalarType::F64),
            AcceleratorIrOperation::Float {
                value: value.clone(),
            },
        )),
        CoreExpr::Atom(value) if value == "true" || value == "false" => Ok(node(
            AcceleratorIrType::Bool,
            AcceleratorIrOperation::Bool {
                value: value == "true",
            },
        )),
        CoreExpr::Var(name) if name == "true" || name == "false" => Ok(node(
            AcceleratorIrType::Bool,
            AcceleratorIrOperation::Bool {
                value: name == "true",
            },
        )),
        CoreExpr::Var(name) => locals
            .get(name)
            .cloned()
            .map(|ty| node(ty, AcceleratorIrOperation::Local { name: name.clone() }))
            .ok_or_else(|| AcceleratorIrError::UnknownLocal(name.clone())),
        CoreExpr::Let { bindings, body } => {
            let mut scoped = locals.clone();
            let mut lowered = Vec::new();
            for binding in bindings {
                let CorePattern::Var(name) = &binding.pattern else {
                    return Err(AcceleratorIrError::UnsupportedOperation(
                        "destructuring let".to_string(),
                    ));
                };
                let value = lower_expr(&binding.value, selection, &mut scoped)?;
                scoped.insert(name.clone(), value.ty.clone());
                lowered.push((name.clone(), value));
            }
            let body = lower_expr(body, selection, &mut scoped)?;
            Ok(node(
                body.ty.clone(),
                AcceleratorIrOperation::Let {
                    bindings: lowered,
                    body: Box::new(body),
                },
            ))
        }
        CoreExpr::UnaryOp { operator, operand } => {
            let operand = lower_expr(operand, selection, locals)?;
            let operation = match operator.as_str() {
                "-" => AcceleratorIrUnaryOperation::Negate,
                "not" | "!" => AcceleratorIrUnaryOperation::Not,
                _ => return Err(AcceleratorIrError::UnsupportedOperation(operator.clone())),
            };
            let ty = operand.ty.clone();
            Ok(node(
                ty,
                AcceleratorIrOperation::Unary {
                    operation,
                    operand: Box::new(operand),
                },
            ))
        }
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => lower_binary(operator, left, right, selection, locals),
        CoreExpr::If { clauses } => lower_if(clauses, selection, locals),
        CoreExpr::Case { scrutinee, clauses } => lower_case(scrutinee, clauses, selection, locals),
        CoreExpr::Index { base, index } => {
            let CoreExpr::Var(buffer) = base.as_ref() else {
                return Err(AcceleratorIrError::UnsupportedOperation(
                    "computed buffer base".to_string(),
                ));
            };
            let ty = buffer_element_type(locals.get(buffer), false, buffer)?;
            let index = lower_expr(index, selection, locals)?;
            require_integer(&index.ty)?;
            Ok(node(
                ty,
                AcceleratorIrOperation::Load {
                    buffer: buffer.clone(),
                    index: Box::new(index),
                },
            ))
        }
        CoreExpr::MutableReceiverCall {
            receiver,
            method,
            args,
            effects,
        } if method == "set" && effects.effects.is_empty() => {
            let CoreExpr::Var(buffer) = receiver.as_ref() else {
                return Err(AcceleratorIrError::UnsupportedOperation(
                    "computed store base".to_string(),
                ));
            };
            let [index, value] = args.as_slice() else {
                return Err(AcceleratorIrError::UnsupportedOperation(
                    "buffer.set arity".to_string(),
                ));
            };
            let expected = buffer_element_type(locals.get(buffer), true, buffer)?;
            let index = lower_expr(index, selection, locals)?;
            require_integer(&index.ty)?;
            let value = lower_expr(value, selection, locals)?;
            require_same_type(&expected, &value.ty, "buffer store")?;
            Ok(node(
                AcceleratorIrType::Unit,
                AcceleratorIrOperation::Store {
                    buffer: buffer.clone(),
                    index: Box::new(index),
                    value: Box::new(value),
                },
            ))
        }
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } => {
            let operation = format!("{module}.{function}");
            if !selection.math_operations.contains(&operation) {
                return Err(AcceleratorIrError::MissingPackageOperation(operation));
            }
            let arguments = args
                .iter()
                .map(|arg| lower_expr(arg, selection, locals))
                .collect::<Result<Vec<_>, _>>()?;
            let ty = arguments
                .first()
                .map(|argument| argument.ty.clone())
                .ok_or_else(|| AcceleratorIrError::TypeMismatch(operation.clone()))?;
            if arguments.iter().any(|argument| argument.ty != ty) {
                return Err(AcceleratorIrError::TypeMismatch(operation));
            }
            Ok(node(
                ty,
                AcceleratorIrOperation::Math {
                    operation,
                    arguments,
                },
            ))
        }
        CoreExpr::Call { function, .. } => Err(AcceleratorIrError::DynamicCall(function.clone())),
        CoreExpr::FunctionCall { .. } | CoreExpr::RemoteFunRef { .. } => Err(
            AcceleratorIrError::DynamicCall("dynamic function value".to_string()),
        ),
        CoreExpr::Intrinsic(call) => Err(AcceleratorIrError::UnsupportedEffect(format!(
            "intrinsic {:?} effects {:?}",
            call.id, call.effects.effects
        ))),
        CoreExpr::Try { .. } => Err(AcceleratorIrError::UnsupportedEffect(
            "exceptions".to_string(),
        )),
        CoreExpr::Lam { .. } => Err(AcceleratorIrError::UnsupportedEffect("closure".to_string())),
        CoreExpr::List(_)
        | CoreExpr::ListCons { .. }
        | CoreExpr::ListComprehension { .. }
        | CoreExpr::Map(_)
        | CoreExpr::RecordConstruct { .. }
        | CoreExpr::RecordUpdate { .. }
        | CoreExpr::ConstructorCall { .. }
        | CoreExpr::ConstructorChain { .. }
        | CoreExpr::FixedArray(_) => Err(AcceleratorIrError::UnsupportedEffect(
            "allocation".to_string(),
        )),
        unsupported => Err(AcceleratorIrError::UnsupportedOperation(format!(
            "{unsupported:?}"
        ))),
    }
}

/// Lowers arithmetic, comparison, and Boolean operators.
fn lower_binary(
    operator: &str,
    left: &CoreExpr,
    right: &CoreExpr,
    selection: &AcceleratorKernelSelection,
    locals: &mut BTreeMap<String, AcceleratorIrType>,
) -> Result<AcceleratorIrNode, AcceleratorIrError> {
    let left = lower_expr(left, selection, locals)?;
    let right = lower_expr(right, selection, locals)?;
    require_same_type(&left.ty, &right.ty, operator)?;
    let source = selection.source.clone();
    let arithmetic = match operator {
        "+" => Some(AcceleratorIrBinaryOperation::Add),
        "-" => Some(AcceleratorIrBinaryOperation::Subtract),
        "*" => Some(AcceleratorIrBinaryOperation::Multiply),
        "/" => Some(AcceleratorIrBinaryOperation::Divide),
        "%" | "rem" => Some(AcceleratorIrBinaryOperation::Remainder),
        "and" => Some(AcceleratorIrBinaryOperation::And),
        "or" => Some(AcceleratorIrBinaryOperation::Or),
        _ => None,
    };
    if let Some(operation) = arithmetic {
        return Ok(AcceleratorIrNode {
            ty: left.ty.clone(),
            source,
            operation: AcceleratorIrOperation::Binary {
                operation,
                left: Box::new(left),
                right: Box::new(right),
            },
        });
    }
    let comparison = match operator {
        "==" | "=:=" => AcceleratorIrComparison::Equal,
        "!=" | "=/=" => AcceleratorIrComparison::NotEqual,
        "<" => AcceleratorIrComparison::Less,
        "<=" | "=<" => AcceleratorIrComparison::LessEqual,
        ">" => AcceleratorIrComparison::Greater,
        ">=" => AcceleratorIrComparison::GreaterEqual,
        _ => {
            return Err(AcceleratorIrError::UnsupportedOperation(
                operator.to_string(),
            ))
        }
    };
    Ok(AcceleratorIrNode {
        ty: AcceleratorIrType::Bool,
        source,
        operation: AcceleratorIrOperation::Compare {
            comparison,
            left: Box::new(left),
            right: Box::new(right),
        },
    })
}

/// Lowers source `if` clauses into nested structured branches.
fn lower_if(
    clauses: &[crate::compiler::typeck::CoreIfClause],
    selection: &AcceleratorKernelSelection,
    locals: &mut BTreeMap<String, AcceleratorIrType>,
) -> Result<AcceleratorIrNode, AcceleratorIrError> {
    let mut fallback = None;
    for clause in clauses.iter().rev() {
        let condition = lower_expr(&clause.condition, selection, locals)?;
        let body = lower_expr(&clause.body, selection, locals)?;
        if matches!(
            condition.operation,
            AcceleratorIrOperation::Bool { value: true }
        ) && fallback.is_none()
        {
            fallback = Some(body);
            continue;
        }
        require_same_type(&condition.ty, &AcceleratorIrType::Bool, "if condition")?;
        let else_value = fallback.take().ok_or_else(|| {
            AcceleratorIrError::UnsupportedOperation("if without fallback".to_string())
        })?;
        require_same_type(&body.ty, &else_value.ty, "if branches")?;
        fallback = Some(AcceleratorIrNode {
            ty: body.ty.clone(),
            source: selection.source.clone(),
            operation: AcceleratorIrOperation::If {
                condition: Box::new(condition),
                then_value: Box::new(body),
                else_value: Box::new(else_value),
            },
        });
    }
    fallback.ok_or_else(|| AcceleratorIrError::UnsupportedOperation("empty if".to_string()))
}

/// Lowers scalar case clauses into nested comparisons and branches.
fn lower_case(
    scrutinee: &CoreExpr,
    clauses: &[crate::compiler::typeck::CoreCaseClause],
    selection: &AcceleratorKernelSelection,
    locals: &mut BTreeMap<String, AcceleratorIrType>,
) -> Result<AcceleratorIrNode, AcceleratorIrError> {
    let scrutinee = lower_expr(scrutinee, selection, locals)?;
    let mut fallback = None;
    for clause in clauses.iter().rev() {
        if clause.guard.is_some() {
            return Err(AcceleratorIrError::UnsupportedOperation(
                "case guard".to_string(),
            ));
        }
        let body = lower_expr(&clause.body, selection, locals)?;
        if matches!(clause.pattern, CorePattern::Wildcard | CorePattern::Var(_)) {
            fallback = Some(body);
            continue;
        }
        let pattern = pattern_value(&clause.pattern, selection)?;
        require_same_type(&scrutinee.ty, &pattern.ty, "case pattern")?;
        let else_value = fallback.take().ok_or_else(|| {
            AcceleratorIrError::UnsupportedOperation("case without fallback".to_string())
        })?;
        require_same_type(&body.ty, &else_value.ty, "case branches")?;
        fallback = Some(AcceleratorIrNode {
            ty: body.ty.clone(),
            source: selection.source.clone(),
            operation: AcceleratorIrOperation::If {
                condition: Box::new(AcceleratorIrNode {
                    ty: AcceleratorIrType::Bool,
                    source: selection.source.clone(),
                    operation: AcceleratorIrOperation::Compare {
                        comparison: AcceleratorIrComparison::Equal,
                        left: Box::new(scrutinee.clone()),
                        right: Box::new(pattern),
                    },
                }),
                then_value: Box::new(body),
                else_value: Box::new(else_value),
            },
        });
    }
    fallback.ok_or_else(|| AcceleratorIrError::UnsupportedOperation("empty case".to_string()))
}

/// Converts one scalar CoreIR pattern to an AcceleratorIR literal.
fn pattern_value(
    pattern: &CorePattern,
    selection: &AcceleratorKernelSelection,
) -> Result<AcceleratorIrNode, AcceleratorIrError> {
    let expr = match pattern {
        CorePattern::Int(value) => CoreExpr::Int(*value),
        CorePattern::Float(value) => CoreExpr::Float(value.clone()),
        CorePattern::Atom(value) if value == "true" || value == "false" => {
            CoreExpr::Atom(value.clone())
        }
        _ => {
            return Err(AcceleratorIrError::UnsupportedOperation(
                "non-scalar case pattern".to_string(),
            ))
        }
    };
    lower_expr(&expr, selection, &mut BTreeMap::new())
}

/// Maps scalar CoreIR types to the canonical accelerator dtype model.
fn lower_core_type(ty: &CoreType) -> Result<AcceleratorIrType, AcceleratorIrError> {
    match ty {
        CoreType::Int => Ok(scalar(AcceleratorScalarType::I64)),
        CoreType::Float | CoreType::Number => Ok(scalar(AcceleratorScalarType::F64)),
        CoreType::Bool => Ok(AcceleratorIrType::Bool),
        CoreType::Named(name) if name == "Unit" => Ok(AcceleratorIrType::Unit),
        _ => Err(AcceleratorIrError::UnsupportedType(format!("{ty:?}"))),
    }
}

/// Returns one canonical scalar IR type.
fn scalar(dtype: AcceleratorScalarType) -> AcceleratorIrType {
    AcceleratorIrType::Scalar { dtype }
}

/// Returns a buffer element type after validating mutation access.
fn buffer_element_type(
    ty: Option<&AcceleratorIrType>,
    write: bool,
    name: &str,
) -> Result<AcceleratorIrType, AcceleratorIrError> {
    let Some(AcceleratorIrType::Buffer { dtype, access, .. }) = ty else {
        return Err(AcceleratorIrError::UnsupportedType(name.to_string()));
    };
    if write && *access == AcceleratorIrAccess::Read {
        return Err(AcceleratorIrError::UnsupportedEffect(format!(
            "write to read-only buffer `{name}`"
        )));
    }
    if !write && *access == AcceleratorIrAccess::Write {
        return Err(AcceleratorIrError::UnsupportedEffect(format!(
            "read from write-only buffer `{name}`"
        )));
    }
    Ok(scalar(*dtype))
}

/// Requires an integer index type.
fn require_integer(ty: &AcceleratorIrType) -> Result<(), AcceleratorIrError> {
    if matches!(
        ty,
        AcceleratorIrType::Scalar {
            dtype: AcceleratorScalarType::I8
                | AcceleratorScalarType::I16
                | AcceleratorScalarType::I32
                | AcceleratorScalarType::I64
                | AcceleratorScalarType::U8
                | AcceleratorScalarType::U16
                | AcceleratorScalarType::U32
                | AcceleratorScalarType::U64
        }
    ) {
        Ok(())
    } else {
        Err(AcceleratorIrError::TypeMismatch("buffer index".to_string()))
    }
}

/// Requires exact first-subset type equality.
fn require_same_type(
    left: &AcceleratorIrType,
    right: &AcceleratorIrType,
    context: &str,
) -> Result<(), AcceleratorIrError> {
    if left == right {
        Ok(())
    } else {
        Err(AcceleratorIrError::TypeMismatch(context.to_string()))
    }
}

#[cfg(test)]
#[path = "ir_test.rs"]
mod ir_test;
