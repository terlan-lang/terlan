/// Static file emitted by a binding generator probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct GeneratedFile {
    pub(super) path: &'static str,
    pub(super) contents: &'static str,
}

const POLARS_TOML: &str = r#"[package]
name = "std-native-polars"
version = "0.0.4"
namespace = "std.native.polars"

[build]
source_roots = ["src"]
artifact = "library"

[target.rust.dependencies]
polars = { cargo = "polars", version = "0.54.4", features = ["lazy", "csv", "strings"] }
"#;

const POLARS_DATAFRAME_TN: &str = r#"/**
 * Native Polars DataFrame contract.
 *
 * `std.native.polars.DataFrame` is the first curated native Rust package probe.
 * It is intentionally small and opaque: Terlan source can pass DataFrame values
 * through typed APIs, while the native Rust adapter owns the real Polars value.
 */

module std.native.polars.DataFrame.

import std.collections.List.
import std.core.Error.{Error, new}.
import std.core.Result.{Err}.
import type std.collections.List.
import type std.core.Result.

/**
 * NativeUnavailable is the temporary error code used until the Rust adapter
 * exists.
 *
 * Input: no runtime input.
 * Output: singleton atom alias for unavailable native package behavior.
 * Transformation: gives stubbed functions a stable typed error value without
 * exposing any target-specific exception shape.
 */
pub type NativeUnavailable =
    Atom["native_unavailable"].

/**
 * DataFrame represents an opaque Polars data frame value.
 *
 * Input: no direct Terlan construction input.
 * Output: an opaque handle type whose representation is owned by the native
 * Rust adapter.
 * Transformation: prevents Terlan source from depending on Polars internals
 * while allowing typed calls across package boundaries.
 */
pub opaque type DataFrame.

/**
 * Reads a CSV file into a Polars data frame.
 *
 * Input: one filesystem path.
 * Output: `Ok(DataFrame)` when the native adapter can read the file, otherwise
 * `Err(Error)`.
 * Transformation: currently returns a stable unavailable-native error; the
 * Rust adapter slice will lower this declaration to `polars::prelude` calls.
 *
 * @example target rust
 * > read_csv("data.csv").
 */
pub read_csv(_path: String): Result[DataFrame, Error] ->
    Err(new(NativeUnavailable, "std.native.polars requires the Rust native target adapter")).

/**
 * Returns the number of rows in a data frame.
 *
 * Input: one opaque `DataFrame` receiver.
 * Output: row count.
 * Transformation: currently returns `0` as a declaration stub; the Rust adapter
 * slice will forward this to the underlying Polars DataFrame.
 */
pub (_df: DataFrame) height(): Int ->
    0.

/**
 * Returns the number of columns in a data frame.
 *
 * Input: one opaque `DataFrame` receiver.
 * Output: column count.
 * Transformation: currently returns `0` as a declaration stub; the Rust adapter
 * slice will forward this to the underlying Polars DataFrame.
 */
pub (_df: DataFrame) width(): Int ->
    0.

/**
 * Returns the column names in a data frame.
 *
 * Input: one opaque `DataFrame` receiver.
 * Output: a `List[String]` containing column names in data-frame order.
 * Transformation: currently returns an empty list as a declaration stub; the
 * Rust adapter slice will copy column names from Polars into Terlan strings.
 */
pub (_df: DataFrame) columns(): List[String] ->
    List.new().

/**
 * Selects a subset of columns from a data frame.
 *
 * Input: one opaque `DataFrame` receiver and a list of column names.
 * Output: `Ok(DataFrame)` for a selected frame, otherwise `Err(Error)`.
 * Transformation: currently returns a stable unavailable-native error; the
 * Rust adapter slice will lower this to a curated Polars selection operation.
 */
pub (_df: DataFrame) select(_columns: List[String]): Result[DataFrame, Error] ->
    Err(new(NativeUnavailable, "std.native.polars requires the Rust native target adapter")).
"#;

const POLARS_MAPPING_TOML: &str = r#"[package]
terlan = "std.native.polars"
cargo = "polars"
version = "0.54.4"
features = ["lazy", "csv", "strings"]

[types]
DataFrame = { rust = "polars::prelude::DataFrame", terlan = "std.native.polars.DataFrame.DataFrame", opaque = true }

[errors]
Error = { rust = "TerlanPolarsError", terlan = "std.core.Error.Error", conversion = "code_message" }

[functions.read_csv]
terlan = "std.native.polars.DataFrame.read_csv"
rust = "polars::prelude::CsvReadOptions"
error = "std.core.Error.Error"
status = "stub"

[methods.height]
receiver = "DataFrame"
terlan = "height"
rust = "DataFrame::height"
status = "stub"

[methods.width]
receiver = "DataFrame"
terlan = "width"
rust = "DataFrame::width"
status = "stub"

[methods.columns]
receiver = "DataFrame"
terlan = "columns"
rust = "DataFrame::get_column_names"
status = "stub"

[methods.select]
receiver = "DataFrame"
terlan = "select"
rust = "DataFrame::select"
error = "std.core.Error.Error"
status = "stub"
"#;

const POLARS_NATIVE_ABI_TOML: &str = r#"[package]
namespace = "std.native.polars"
adapter = "rust"
crate = "std-native-polars-adapter"
status = "stub"

[runtime]
bridge = "supervised_actor"
worker = "rust_thread_probe"
ownership = "opaque_handles"
backpressure = "credit"
shared_memory = false
handle_generation_tokens = true
explicit_disposal = true

[runtime.commands]
start = "start_worker"
call = "typed_request"
stop = "stop_worker"

[runtime.beam]
supervision = "std.vm.NativeBridge.NativeBridgeRuntime"
process = "std.vm.Process.Process"
message = "std.vm.Message.MessageCodec"
backpressure = "std.vm.Backpressure.Backpressure"
credit = "std.vm.Backpressure.Credit"

[types."std.native.polars.DataFrame.DataFrame"]
rust = "TerlanPolarsDataFrame"
ownership = "opaque"

[errors."std.core.Error.Error"]
rust = "TerlanPolarsError"
conversion = "code_message"
code = "code"
message = "message"
native_unavailable_code = "native_unavailable"
native_unavailable_message = "std.native.polars requires the Rust native target adapter"

[functions."std.native.polars.DataFrame.read_csv"]
rust = "read_csv"
inputs = ["String"]
output = "Result[DataFrame, Error]"
error = "std.core.Error.Error"

[methods."std.native.polars.DataFrame.height"]
rust = "height"
receiver = "DataFrame"
inputs = []
output = "Int"

[methods."std.native.polars.DataFrame.width"]
rust = "width"
receiver = "DataFrame"
inputs = []
output = "Int"

[methods."std.native.polars.DataFrame.columns"]
rust = "columns"
receiver = "DataFrame"
inputs = []
output = "List[String]"

[methods."std.native.polars.DataFrame.select"]
rust = "select"
receiver = "DataFrame"
inputs = ["List[String]"]
output = "Result[DataFrame, Error]"
error = "std.core.Error.Error"

[result_conversions."std.native.polars.DataFrame.read_csv"]
ok = "std.native.polars.DataFrame.DataFrame"
err = "std.core.Error.Error"

[result_conversions."std.native.polars.DataFrame.select"]
ok = "std.native.polars.DataFrame.DataFrame"
err = "std.core.Error.Error"
"#;

const POLARS_PACKAGE_DOC: &str = r#"# std.native.polars

`std.native.polars` is Terlan's first curated native Rust package probe. It is
not part of portable `std.collections`; it exists to validate how Terlan
packages can wrap external Rust crates through an explicit native target.

## Current Status

The package skeleton is generated by:

```sh
terlc bind native --crate polars --out packages/std/native/polars
```

In the current native-package probe slice:

- the Terlan `DataFrame` API is declared and documented;
- the `.typi` interface summary is generated;
- the Rust adapter crate compiles and tests offline;
- Polars crate linkage is recorded as metadata only;
- the internal Vm migration build rejects `std.native.*` modules and imports.

Real Polars execution requires the future Rust/native target capability.

## Example Shape

```terlan
module examples.polars.ReadCsv.

import std.native.polars.DataFrame.{read_csv}.
import std.core.Result.{Err, Ok}.

pub load(path: String): Unit ->
    case read_csv(path) {
        Ok(_df) ->
            Unit;

        Err(_error) ->
            Unit
    }.
```

This example documents the source shape only. It is not executable on the
Vm target because `std.native.polars` requires the Rust/native target.
"#;

const POLARS_READ_CSV_EXAMPLE: &str = r#"module examples.polars.ReadCsv.

import std.native.polars.DataFrame.{read_csv}.
import std.core.Result.{Err, Ok}.

pub load(path: String): Unit ->
    case read_csv(path) {
        Ok(_df) ->
            Unit;

        Err(_error) ->
            Unit
    }.
"#;

const POLARS_DATAFRAME_TYPI: &str = r#"//! Native Polars DataFrame contract.
//!
//! `std.native.polars.DataFrame` is the first curated native Rust package probe.
//! It is intentionally small and opaque: Terlan source can pass DataFrame values
//! through typed APIs, while the native Rust adapter owns the real Polars value.

module std.native.polars.DataFrame.

/// DataFrame represents an opaque Polars data frame value.
///
/// Input: no direct Terlan construction input.
/// Output: an opaque handle type whose representation is owned by the native
/// Rust adapter.
/// Transformation: prevents Terlan source from depending on Polars internals
/// while allowing typed calls across package boundaries.

pub opaque type DataFrame.

/// NativeUnavailable is the temporary error code used until the Rust adapter
/// exists.
///
/// Input: no runtime input.
/// Output: singleton atom alias for unavailable native package behavior.
/// Transformation: gives stubbed functions a stable typed error value without
/// exposing any target-specific exception shape.

pub type NativeUnavailable =
    Atom["native_unavailable"].

/// Returns the column names in a data frame.
///
/// Input: one opaque `DataFrame` receiver.
/// Output: a `List[String]` containing column names in data-frame order.
/// Transformation: currently returns an empty list as a declaration stub; the
/// Rust adapter slice will copy column names from Polars into Terlan strings.

pub (df: DataFrame) columns(): List[String].

/// Returns the number of rows in a data frame.
///
/// Input: one opaque `DataFrame` receiver.
/// Output: row count.
/// Transformation: currently returns `0` as a declaration stub; the Rust adapter
/// slice will forward this to the underlying Polars DataFrame.

pub (df: DataFrame) height(): Int.

/// Reads a CSV file into a Polars data frame.
///
/// Input: one filesystem path.
/// Output: `Ok(DataFrame)` when the native adapter can read the file, otherwise
/// `Err(Error)`.
/// Transformation: currently returns a stable unavailable-native error; the
/// Rust adapter slice will lower this declaration to `polars::prelude` calls.

pub read_csv(path: String): Result[DataFrame, Error].

/// Selects a subset of columns from a data frame.
///
/// Input: one opaque `DataFrame` receiver and a list of column names.
/// Output: `Ok(DataFrame)` for a selected frame, otherwise `Err(Error)`.
/// Transformation: currently returns a stable unavailable-native error; the
/// Rust adapter slice will lower this to a curated Polars selection operation.

pub (df: DataFrame) select(columns: List[String]): Result[DataFrame, Error].

/// Returns the number of columns in a data frame.
///
/// Input: one opaque `DataFrame` receiver.
/// Output: column count.
/// Transformation: currently returns `0` as a declaration stub; the Rust adapter
/// slice will forward this to the underlying Polars DataFrame.

pub (df: DataFrame) width(): Int.
"#;

const POLARS_RUST_CARGO_TOML: &str = r#"[package]
name = "std-native-polars-adapter"
version = "0.0.4"
edition = "2021"

[lib]
path = "src/lib.rs"

[package.metadata.terlan.polars]
cargo = "polars"
version = "0.54.4"
features = ["lazy", "csv", "strings"]
link_status = "deferred"

[workspace]
"#;

const POLARS_RUST_STUB: &str = r#"#![forbid(unsafe_code)]
//! Rust adapter skeleton for `std.native.polars`.
//!
//! Inputs:
//! - Opaque Terlan DataFrame handles supplied by the future native package ABI.
//! - Curated Polars operations selected by `bindings/polars.mapping.toml`.
//!
//! Outputs:
//! - Native adapter functions that translate between Terlan package calls and
//!   `polars` crate values.
//!
//! Transformation:
//! - This file is intentionally a stub until the Rust native target ABI links
//!   Polars. It records callable adapter boundaries without depending on the
//!   upstream crate.

pub mod bridge;

/// Native DataFrame handle placeholder.
///
/// Inputs:
/// - None.
///
/// Outputs:
/// - A cloneable Rust marker used only by the package skeleton.
///
/// Transformation:
/// - Reserves the adapter-side handle name while the real Polars ownership and
///   lifetime contract is designed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerlanPolarsDataFrame;

/// Native Polars adapter error placeholder.
///
/// Inputs:
/// - Static error code and message supplied by adapter functions.
///
/// Outputs:
/// - A typed Rust error value that can later map into `std.core.Error.Error`.
///
/// Transformation:
/// - Keeps adapter failures explicit while the real Polars error conversion is
///   still pending.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerlanPolarsError {
    pub code: &'static str,
    pub message: &'static str,
}

impl TerlanPolarsError {
    /// Returns the Terlan error-code atom name.
    ///
    /// Inputs:
    /// - `self`: adapter error value.
    ///
    /// Outputs:
    /// - Stable atom-name text without target-specific exception data.
    ///
    /// Transformation:
    /// - Exposes the first half of the `std.core.Error.Error` conversion shape.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Returns the Terlan error message.
    ///
    /// Inputs:
    /// - `self`: adapter error value.
    ///
    /// Outputs:
    /// - Stable UTF-8 message text for `std.core.Error.Error`.
    ///
    /// Transformation:
    /// - Exposes the second half of the `std.core.Error.Error` conversion
    ///   shape.
    pub fn message(&self) -> &'static str {
        self.message
    }

    /// Splits the adapter error into the future Terlan ABI fields.
    ///
    /// Inputs:
    /// - `self`: adapter error value.
    ///
    /// Outputs:
    /// - `(code, message)` tuple matching `native/terlan-native.toml`.
    ///
    /// Transformation:
    /// - Makes error conversion testable before the native target links real
    ///   Polars errors.
    pub fn into_parts(self) -> (&'static str, &'static str) {
        (self.code, self.message)
    }
}

/// Builds the current unavailable-native adapter error.
///
/// Inputs:
/// - None.
///
/// Outputs:
/// - `TerlanPolarsError` with stable code and message fields.
///
/// Transformation:
/// - Centralizes the temporary error returned by stubbed adapter functions.
fn unavailable_error() -> TerlanPolarsError {
    TerlanPolarsError {
        code: "native_unavailable",
        message: "std.native.polars requires the Rust native target adapter",
    }
}

/// Reads a CSV file into a native DataFrame.
///
/// Inputs:
/// - `path`: UTF-8 filesystem path supplied by Terlan.
///
/// Outputs:
/// - `Ok(TerlanPolarsDataFrame)` once the real Polars adapter is linked.
/// - `Err(TerlanPolarsError)` in the current stub implementation.
///
/// Transformation:
/// - Reserves the Rust function boundary for
///   `std.native.polars.DataFrame.read_csv`.
pub fn read_csv(_path: &str) -> Result<TerlanPolarsDataFrame, TerlanPolarsError> {
    Err(unavailable_error())
}

/// Returns a DataFrame row count.
///
/// Inputs:
/// - `df`: native DataFrame handle.
///
/// Outputs:
/// - Row count as `usize`.
///
/// Transformation:
/// - Reserves the Rust function boundary for the Terlan `height` receiver
///   method while returning the current stub value.
pub fn height(_df: &TerlanPolarsDataFrame) -> usize {
    0
}

/// Returns a DataFrame column count.
///
/// Inputs:
/// - `df`: native DataFrame handle.
///
/// Outputs:
/// - Column count as `usize`.
///
/// Transformation:
/// - Reserves the Rust function boundary for the Terlan `width` receiver method
///   while returning the current stub value.
pub fn width(_df: &TerlanPolarsDataFrame) -> usize {
    0
}

/// Returns DataFrame column names.
///
/// Inputs:
/// - `df`: native DataFrame handle.
///
/// Outputs:
/// - Owned UTF-8 column names.
///
/// Transformation:
/// - Reserves the Rust function boundary for the Terlan `columns` receiver
///   method while returning the current stub value.
pub fn columns(_df: &TerlanPolarsDataFrame) -> Vec<String> {
    Vec::new()
}

/// Selects DataFrame columns.
///
/// Inputs:
/// - `df`: native DataFrame handle.
/// - `columns`: UTF-8 column names supplied by Terlan.
///
/// Outputs:
/// - `Ok(TerlanPolarsDataFrame)` once the real Polars adapter is linked.
/// - `Err(TerlanPolarsError)` in the current stub implementation.
///
/// Transformation:
/// - Reserves the Rust function boundary for the Terlan `select` receiver
///   method without exposing Polars internals to Terlan source.
pub fn select(
    _df: &TerlanPolarsDataFrame,
    _columns: &[String],
) -> Result<TerlanPolarsDataFrame, TerlanPolarsError> {
    Err(unavailable_error())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the stubbed read path returns the stable unavailable error.
    ///
    /// Inputs:
    /// - Static CSV path.
    ///
    /// Outputs:
    /// - Test assertions over the returned error fields.
    ///
    /// Transformation:
    /// - Calls the public adapter boundary without linking Polars.
    #[test]
    fn read_csv_returns_unavailable_error() {
        let err = read_csv("data.csv").expect_err("stub should return unavailable error");

        assert_eq!(err.code(), "native_unavailable");
        assert_eq!(
            err.message(),
            "std.native.polars requires the Rust native target adapter"
        );
    }

    /// Verifies adapter errors expose the future Terlan error fields.
    ///
    /// Inputs:
    /// - Static adapter error from the unavailable-native stub.
    ///
    /// Outputs:
    /// - Test assertions over `(code, message)` conversion fields.
    ///
    /// Transformation:
    /// - Exercises the explicit error conversion contract recorded in
    ///   `native/terlan-native.toml`.
    #[test]
    fn adapter_error_converts_to_code_message_parts() {
        let (code, message) = unavailable_error().into_parts();

        assert_eq!(code, "native_unavailable");
        assert_eq!(
            message,
            "std.native.polars requires the Rust native target adapter"
        );
    }

    /// Verifies the stubbed DataFrame observers are callable.
    ///
    /// Inputs:
    /// - Placeholder native DataFrame handle.
    ///
    /// Outputs:
    /// - Test assertions over stable stub values.
    ///
    /// Transformation:
    /// - Calls receiver-style adapter functions without linking Polars.
    #[test]
    fn dataframe_observers_return_stub_values() {
        let df = TerlanPolarsDataFrame;

        assert_eq!(height(&df), 0);
        assert_eq!(width(&df), 0);
        assert!(columns(&df).is_empty());
    }
}
"#;
