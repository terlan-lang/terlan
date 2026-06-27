use super::types::{WasmAbiType, WasmValue};

/// WebAssembly function ABI.
///
/// Inputs:
/// - Produced by future Terlan export lowering or host import declarations.
///
/// Output:
/// - Stable parameter/result ABI contract for Wasm functions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmFunctionAbi {
    pub name: String,
    pub params: Vec<WasmAbiType>,
    pub results: Vec<WasmAbiType>,
}

impl WasmFunctionAbi {
    /// Builds a new function ABI.
    pub fn new(
        name: impl Into<String>,
        params: Vec<WasmAbiType>,
        results: Vec<WasmAbiType>,
    ) -> Self {
        Self {
            name: name.into(),
            params,
            results,
        }
    }
}

/// ABI validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WasmAbiError {
    EmptyFunctionName,
    UnsupportedMultiValueResult {
        function: String,
        count: usize,
    },
    MissingResult {
        function: String,
    },
    UnexpectedResult {
        function: String,
    },
    ResultTypeMismatch {
        function: String,
        expected: WasmAbiType,
        actual: WasmAbiType,
    },
}

impl std::fmt::Display for WasmAbiError {
    /// Formats an ABI validation error.
    ///
    /// Inputs: formatter sink.
    /// Output: formatting result.
    /// Transformation: maps each error variant to a stable diagnostic string.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyFunctionName => write!(f, "Wasm ABI function name cannot be empty"),
            Self::UnsupportedMultiValueResult { function, count } => write!(
                f,
                "Wasm ABI function `{function}` has {count} result values; multi-value results are reserved"
            ),
            Self::MissingResult { function } => {
                write!(f, "Wasm ABI function `{function}` requires one result value")
            }
            Self::UnexpectedResult { function } => {
                write!(f, "Wasm ABI function `{function}` does not declare a result value")
            }
            Self::ResultTypeMismatch {
                function,
                expected,
                actual,
            } => write!(
                f,
                "Wasm ABI function `{function}` result type mismatch: expected {}, got {}",
                expected.as_str(),
                actual.as_str()
            ),
        }
    }
}

impl std::error::Error for WasmAbiError {}

/// Validates one exported result value against a function ABI.
///
/// Inputs:
/// - `abi`: expected function ABI.
/// - `value`: optional scalar value produced by an export smoke.
///
/// Output:
/// - `Ok(())` when the value shape matches the ABI result declaration.
/// - `Err(WasmAbiError)` for missing, unexpected, multi-value, or mismatched
///   result shape.
///
/// Transformation:
/// - Keeps scalar ABI validation explicit before host runners or component
///   adapters add richer value transport.
pub fn validate_export_result_value(
    abi: &WasmFunctionAbi,
    value: Option<WasmValue>,
) -> Result<(), WasmAbiError> {
    if abi.name.trim().is_empty() {
        return Err(WasmAbiError::EmptyFunctionName);
    }
    if abi.results.len() > 1 {
        return Err(WasmAbiError::UnsupportedMultiValueResult {
            function: abi.name.clone(),
            count: abi.results.len(),
        });
    }

    match (abi.results.first().copied(), value) {
        (Some(expected), Some(value)) if expected == value.abi_type() => Ok(()),
        (Some(expected), Some(value)) => Err(WasmAbiError::ResultTypeMismatch {
            function: abi.name.clone(),
            expected,
            actual: value.abi_type(),
        }),
        (Some(_), None) => Err(WasmAbiError::MissingResult {
            function: abi.name.clone(),
        }),
        (None, Some(_)) => Err(WasmAbiError::UnexpectedResult {
            function: abi.name.clone(),
        }),
        (None, None) => Ok(()),
    }
}

#[cfg(test)]
#[path = "abi_test.rs"]
mod abi_test;
