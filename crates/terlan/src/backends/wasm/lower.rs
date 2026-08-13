use std::collections::HashMap;

use crate::terlan_typeck::{
    CoreExportKind, CoreExpr, CoreFunction, CoreModule, CoreParam, CoreType,
};

use super::backend_ir::{
    WasmExport, WasmFunction, WasmFunctionBody, WasmInstruction, WasmModuleIr, WasmParam,
    WasmResultType,
};

/// CoreIR-to-Wasm lowering failure for the first supported subset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WasmLowerError {
    NoExportedFunctions,
    UnsupportedParamType {
        function: String,
        param: String,
        param_type: String,
    },
    UnsupportedReturnType {
        function: String,
        return_type: String,
    },
    UnsupportedClauseCount {
        function: String,
        count: usize,
    },
    MissingCoreBody {
        function: String,
    },
    UnsupportedBody {
        function: String,
        body: String,
    },
    UnknownLocal {
        function: String,
        name: String,
    },
    IntegerOutOfRange {
        function: String,
        value: i64,
    },
    InvalidFloatLiteral {
        function: String,
        value: String,
        target: WasmResultType,
    },
}

impl std::fmt::Display for WasmLowerError {
    /// Formats a CoreIR-to-Wasm lowering error.
    ///
    /// Inputs: formatter sink.
    /// Output: formatting result.
    /// Transformation: maps each lowering error to stable diagnostic text.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoExportedFunctions => write!(f, "Wasm lowering requires at least one exported function"),
            Self::UnsupportedParamType {
                function,
                param,
                param_type,
            } => write!(
                f,
                "Wasm lowering supports only Int parameters for now; `{function}` parameter `{param}` has type {param_type}"
            ),
            Self::UnsupportedReturnType {
                function,
                return_type,
            } => write!(
                f,
                "Wasm lowering supports only Int and Bool return values for now; `{function}` returns {return_type}"
            ),
            Self::UnsupportedClauseCount { function, count } => write!(
                f,
                "Wasm lowering supports only single-clause exports for now; `{function}` has {count} clauses"
            ),
            Self::MissingCoreBody { function } => {
                write!(f, "Wasm lowering requires a typed CoreIR body for `{function}`")
            }
            Self::UnsupportedBody { function, body } => write!(
                f,
                "Wasm lowering supports only i32 literals, locals, arithmetic, and comparison expressions for now; `{function}` body is {body}"
            ),
            Self::UnknownLocal { function, name } => write!(
                f,
                "Wasm lowering could not resolve local `{name}` in `{function}`"
            ),
            Self::IntegerOutOfRange { function, value } => write!(
                f,
                "Wasm lowering maps Int literals to i32 for now; `{function}` value {value} is out of range"
            ),
            Self::InvalidFloatLiteral {
                function,
                value,
                target,
            } => write!(
                f,
                "unsupported_abi_signature: `{function}` float literal `{value}` cannot be represented as {}",
                wasm_type_name(*target)
            ),
        }
    }
}

impl std::error::Error for WasmLowerError {}

/// Lowers the first supported checked CoreIR subset into Wasm backend IR.
///
/// Inputs:
/// - `core`: checked CoreIR module from the formal compiler pipeline.
///
/// Output:
/// - Minimal Wasm backend IR containing exported i32 functions, or a stable
///   lowering diagnostic.
///
/// Transformation:
/// - Selects public function exports from CoreIR, validates the first narrow
///   pure subset, and converts `Int` literals, locals, arithmetic, and
///   comparison expressions into exported Wasm i32 instruction streams. It
///   intentionally does not parse source text or emit bytes directly.
pub fn lower_core_module(core: &CoreModule) -> Result<WasmModuleIr, WasmLowerError> {
    let mut functions = Vec::new();
    for export in &core.exports {
        let CoreExportKind::Function { arity } = export.kind else {
            continue;
        };
        let Some(function) = core
            .functions
            .iter()
            .find(|function| function.name == export.name && function.arity == arity)
        else {
            continue;
        };
        functions.push(lower_exported_function(function)?);
    }

    if functions.is_empty() {
        return Err(WasmLowerError::NoExportedFunctions);
    }

    Ok(WasmModuleIr::new(functions))
}

/// Lowers one exported CoreIR function into a Wasm function.
///
/// Inputs:
/// - `function`: public CoreIR function selected through module exports.
///
/// Output:
/// - Exported Wasm function IR or stable unsupported-subset diagnostic.
///
/// Transformation:
/// - Enforces the initial `Int` parameter, single-clause, `Int` expression
///   subset and maps it into an i32 instruction body.
fn lower_exported_function(function: &CoreFunction) -> Result<WasmFunction, WasmLowerError> {
    let Some(result) = wasm_return_type(function.core_return_type.as_ref()) else {
        return Err(WasmLowerError::UnsupportedReturnType {
            function: function.name.clone(),
            return_type: function.return_type.clone(),
        });
    };
    if function.clauses.len() != 1 {
        return Err(WasmLowerError::UnsupportedClauseCount {
            function: function.name.clone(),
            count: function.clauses.len(),
        });
    }

    let Some(core_expr) = &function.clauses[0].body.core_expr else {
        return Err(WasmLowerError::MissingCoreBody {
            function: function.name.clone(),
        });
    };
    let params = lower_params(function)?;
    let locals = params
        .iter()
        .enumerate()
        .map(|(index, param)| (param.name.clone(), index as u32))
        .collect::<HashMap<_, _>>();
    let mut instructions = Vec::new();
    lower_scalar_expr(
        &function.name,
        core_expr,
        result,
        &locals,
        &mut instructions,
    )?;

    Ok(WasmFunction {
        name: function.name.clone(),
        params,
        result,
        body: WasmFunctionBody::Instructions(instructions),
        export: Some(WasmExport {
            name: function.name.clone(),
        }),
    })
}

/// Lowers CoreIR parameters into Wasm parameter metadata.
///
/// Inputs:
/// - `function`: CoreIR function selected for Wasm export.
///
/// Output:
/// - Ordered Wasm parameters or a stable unsupported-type diagnostic.
///
/// Transformation:
/// - Maps Terlan `Int` parameters to Wasm `i32` parameters and preserves
///   source parameter names for manifests and diagnostics.
fn lower_params(function: &CoreFunction) -> Result<Vec<WasmParam>, WasmLowerError> {
    function
        .params
        .iter()
        .map(|param| lower_param(&function.name, param))
        .collect()
}

/// Lowers one CoreIR parameter into Wasm parameter metadata.
///
/// Inputs:
/// - `function_name`: containing export name.
/// - `param`: CoreIR parameter.
///
/// Output:
/// - Wasm `i32` parameter metadata or unsupported-type diagnostic.
///
/// Transformation:
/// - Uses typed CoreIR parameter metadata instead of reparsing the annotation
///   text.
fn lower_param(function_name: &str, param: &CoreParam) -> Result<WasmParam, WasmLowerError> {
    let Some(ty) = wasm_param_type(param.core_ty.as_ref()) else {
        return Err(WasmLowerError::UnsupportedParamType {
            function: function_name.to_string(),
            param: param.name.clone(),
            param_type: param.ty.clone(),
        });
    };
    Ok(WasmParam {
        name: param.name.clone(),
        ty,
    })
}

/// Returns whether a CoreIR type is representable as the first Wasm scalar ABI.
///
/// Inputs:
/// - `ty`: typed CoreIR parameter metadata.
///
/// Output:
/// - `true` for portable `Int` and explicit `std.wasm.Abi.I32` aliases.
///
/// Transformation:
/// - Keeps explicit WebAssembly ABI types source-level aliases while the
///   backend still emits the same i32 value type.
fn wasm_param_type(ty: Option<&CoreType>) -> Option<WasmResultType> {
    match ty {
        Some(CoreType::Int) => Some(WasmResultType::I32),
        Some(CoreType::Named(name)) => wasm_named_scalar(name),
        _ => None,
    }
}

/// Returns whether a CoreIR return type can lower to one Wasm i32 result.
///
/// Inputs:
/// - `ty`: typed CoreIR return metadata.
///
/// Output:
/// - `true` for `Int`, boolean comparison results, and explicit `I32` ABI
///   aliases.
///
/// Transformation:
/// - Preserves the existing bool-as-i32 convention while allowing Wasm ABI
///   declarations to document exported integer results.
fn wasm_return_type(ty: Option<&CoreType>) -> Option<WasmResultType> {
    match ty {
        Some(CoreType::Int | CoreType::Bool) => Some(WasmResultType::I32),
        Some(CoreType::Named(name)) => wasm_named_scalar(name),
        _ => None,
    }
}

/// Returns whether a CoreIR type names the explicit Wasm i32 ABI alias.
///
/// Inputs:
/// - `ty`: typed CoreIR type metadata.
///
/// Output:
/// - `true` for local or fully qualified `I32` aliases.
///
/// Transformation:
/// - Bridges source import style (`I32`) and fully-qualified summaries
///   (`std.wasm.Abi.I32`) into one backend decision without reparsing imports.
fn wasm_named_scalar(name: &str) -> Option<WasmResultType> {
    match name.strip_prefix("std.wasm.Abi.").unwrap_or(name) {
        "I32" => Some(WasmResultType::I32),
        "I64" => Some(WasmResultType::I64),
        "F32" => Some(WasmResultType::F32),
        "F64" => Some(WasmResultType::F64),
        _ => None,
    }
}

/// Returns the manifest spelling for one lowered WASM scalar type.
fn wasm_type_name(ty: WasmResultType) -> &'static str {
    match ty {
        WasmResultType::I32 => "i32",
        WasmResultType::I64 => "i64",
        WasmResultType::F32 => "f32",
        WasmResultType::F64 => "f64",
    }
}

/// Lowers one CoreIR expression into i32 Wasm instructions.
///
/// Inputs:
/// - `function_name`: containing export name for diagnostics.
/// - `expr`: typed CoreIR expression.
/// - `locals`: parameter/local slot map.
/// - `instructions`: output instruction buffer.
///
/// Output:
/// - Appends Wasm instructions or returns a stable unsupported-subset
///   diagnostic.
///
/// Transformation:
/// - Emits stack-machine order for literals, locals, arithmetic, and
///   comparison binary operators.
fn lower_scalar_expr(
    function_name: &str,
    expr: &CoreExpr,
    result: WasmResultType,
    locals: &HashMap<String, u32>,
    instructions: &mut Vec<WasmInstruction>,
) -> Result<(), WasmLowerError> {
    match expr {
        CoreExpr::Int(value) if result == WasmResultType::I32 => {
            let value = i32::try_from(*value).map_err(|_| WasmLowerError::IntegerOutOfRange {
                function: function_name.to_string(),
                value: *value,
            })?;
            instructions.push(WasmInstruction::I32Const(value));
            Ok(())
        }
        CoreExpr::Int(value) if result == WasmResultType::I64 => {
            instructions.push(WasmInstruction::I64Const(*value));
            Ok(())
        }
        CoreExpr::Float(value) if result == WasmResultType::F32 => {
            let parsed = value
                .parse::<f32>()
                .map_err(|_| WasmLowerError::InvalidFloatLiteral {
                    function: function_name.to_string(),
                    value: value.clone(),
                    target: result,
                })?;
            if !parsed.is_finite() {
                return Err(WasmLowerError::InvalidFloatLiteral {
                    function: function_name.to_string(),
                    value: value.clone(),
                    target: result,
                });
            }
            instructions.push(WasmInstruction::F32ConstBits(parsed.to_bits()));
            Ok(())
        }
        CoreExpr::Float(value) if result == WasmResultType::F64 => {
            let parsed = value
                .parse::<f64>()
                .map_err(|_| WasmLowerError::InvalidFloatLiteral {
                    function: function_name.to_string(),
                    value: value.clone(),
                    target: result,
                })?;
            if !parsed.is_finite() {
                return Err(WasmLowerError::InvalidFloatLiteral {
                    function: function_name.to_string(),
                    value: value.clone(),
                    target: result,
                });
            }
            instructions.push(WasmInstruction::F64ConstBits(parsed.to_bits()));
            Ok(())
        }
        CoreExpr::Var(name) => {
            let Some(index) = locals.get(name) else {
                return Err(WasmLowerError::UnknownLocal {
                    function: function_name.to_string(),
                    name: name.clone(),
                });
            };
            instructions.push(WasmInstruction::LocalGet(*index));
            Ok(())
        }
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } if result == WasmResultType::I32 && matches!(operator.as_str(), "+" | "-" | "*") => {
            lower_scalar_expr(function_name, left, result, locals, instructions)?;
            lower_scalar_expr(function_name, right, result, locals, instructions)?;
            instructions.push(match operator.as_str() {
                "+" => WasmInstruction::I32Add,
                "-" => WasmInstruction::I32Sub,
                "*" => WasmInstruction::I32Mul,
                _ => unreachable!("operator guard accepts only +, -, *"),
            });
            Ok(())
        }
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } if result == WasmResultType::I32
            && matches!(operator.as_str(), "==" | "!=" | "<" | "<=" | ">" | ">=") =>
        {
            lower_scalar_expr(function_name, left, result, locals, instructions)?;
            lower_scalar_expr(function_name, right, result, locals, instructions)?;
            instructions.push(match operator.as_str() {
                "==" => WasmInstruction::I32Eq,
                "!=" => WasmInstruction::I32Ne,
                "<" => WasmInstruction::I32LtS,
                "<=" => WasmInstruction::I32LeS,
                ">" => WasmInstruction::I32GtS,
                ">=" => WasmInstruction::I32GeS,
                _ => unreachable!("operator guard accepts only comparison operators"),
            });
            Ok(())
        }
        _ => Err(WasmLowerError::UnsupportedBody {
            function: function_name.to_string(),
            body: core_expr_kind(expr).to_string(),
        }),
    }
}

/// Returns a stable CoreIR expression kind label for Wasm diagnostics.
///
/// Inputs:
/// - `expr`: typed CoreIR expression.
///
/// Output:
/// - Static expression kind label.
///
/// Transformation:
/// - Avoids depending on summary strings when diagnostics are emitted from
///   direct typed CoreIR payloads.
fn core_expr_kind(expr: &CoreExpr) -> &'static str {
    match expr {
        CoreExpr::Int(_) => "Int",
        CoreExpr::Float(_) => "Float",
        CoreExpr::Binary(_) => "Binary",
        CoreExpr::Atom(_) => "Atom",
        CoreExpr::Var(_) => "Var",
        CoreExpr::Tuple(_) => "Tuple",
        CoreExpr::List(_) => "List",
        CoreExpr::ListCons { .. } => "ListCons",
        CoreExpr::FixedArray(_) => "FixedArray",
        CoreExpr::Index { .. } => "Index",
        CoreExpr::ListComprehension { .. } => "ListComprehension",
        CoreExpr::Let { .. } => "Let",
        CoreExpr::Map(_) => "Map",
        CoreExpr::RecordConstruct { .. } => "RecordConstruct",
        CoreExpr::FieldAccess { .. } => "FieldAccess",
        CoreExpr::RecordAccess { .. } => "RecordAccess",
        CoreExpr::RecordUpdate { .. } => "RecordUpdate",
        CoreExpr::TemplateInstantiate { .. } => "TemplateInstantiate",
        CoreExpr::ConstructorChain { .. } => "ConstructorChain",
        CoreExpr::RemoteFunRef { .. } => "RemoteFunRef",
        CoreExpr::RemoteCall { .. } => "RemoteCall",
        CoreExpr::ConstructorCall { .. } => "ConstructorCall",
        CoreExpr::Call { .. } => "Call",
        CoreExpr::MutableReceiverCall { .. } => "MutableReceiverCall",
        CoreExpr::FunctionCall { .. } => "FunctionCall",
        CoreExpr::Cast { .. } => "Cast",
        CoreExpr::Intrinsic(_) => "Intrinsic",
        CoreExpr::SqlQuery { .. } => "SqlQuery",
        CoreExpr::Case { .. } => "Case",
        CoreExpr::Try { .. } => "Try",
        CoreExpr::If { .. } => "If",
        CoreExpr::Lam { .. } => "Lam",
        CoreExpr::UnaryOp { .. } => "UnaryOp",
        CoreExpr::BinaryOp { .. } => "BinaryOp",
    }
}

#[cfg(test)]
#[path = "lower_test.rs"]
#[cfg(test)]
mod lower_test;
