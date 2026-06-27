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
supervision = "std.beam.NativeBridge.NativeBridgeRuntime"
process = "std.beam.Process.Process"
message = "std.beam.Message.MessageCodec"
backpressure = "std.beam.Backpressure.Backpressure"
credit = "std.beam.Backpressure.Credit"

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
terlc bind rust --crate polars --out packages/std/native/polars
```

In the current native-package probe slice:

- the Terlan `DataFrame` API is declared and documented;
- the `.typi` interface summary is generated;
- the Rust adapter crate compiles and tests offline;
- Polars crate linkage is recorded as metadata only;
- `terlc build --target erlang` rejects `std.native.*` modules and imports.

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
Erlang target because `std.native.polars` requires the Rust/native target.
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

const POLARS_RUST_BRIDGE: &str = r#"#![forbid(unsafe_code)]
//! Supervised native worker probe for `std.native.polars`.
//!
//! Inputs:
//! - Typed bridge commands from the future BEAM/native adapter boundary.
//! - Opaque handles created and owned by the native worker.
//!
//! Outputs:
//! - Typed replies carrying request ids, stable errors, and credit information.
//! - Opaque handle values that BEAM can store without seeing native pointers.
//!
//! Transformation:
//! - Models the Terlan supervised actor bridge without linking Polars or Tokio
//!   yet. The real native target can replace the worker thread with a Tokio
//!   runtime while preserving the same command/reply and handle-generation
//!   contract.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

pub const DEFAULT_CREDIT_WINDOW: usize = 32;
pub const DATAFRAME_TYPE: &str = "std.native.polars.DataFrame.DataFrame";

/// Opaque native handle carried by Terlan/BEAM terms.
///
/// Inputs:
/// - Numeric resource id and generation assigned by the worker.
/// - Stable source-level type name for diagnostics and type checks.
///
/// Outputs:
/// - Copyable handle token with no raw pointer or native storage.
///
/// Transformation:
/// - Separates native ownership from Terlan values while allowing stale-handle
///   detection through generation tokens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeHandle {
    pub id: u64,
    pub generation: u64,
    pub type_name: &'static str,
}

/// Stable native bridge error.
///
/// Inputs:
/// - Static error code and owned message text.
///
/// Outputs:
/// - Error shape suitable for lowering into `std.core.Error.Error`.
///
/// Transformation:
/// - Keeps worker failures target-neutral and independent from Rust panic or
///   transport details.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeBridgeError {
    pub code: &'static str,
    pub message: String,
}

impl NativeBridgeError {
    /// Creates a stable bridge error.
    ///
    /// Inputs:
    /// - `code`: stable machine-readable error code.
    /// - `message`: human-readable diagnostic text.
    ///
    /// Outputs:
    /// - `NativeBridgeError` with owned message storage.
    ///
    /// Transformation:
    /// - Normalizes arbitrary message inputs into the bridge error shape.
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Typed value returned by the worker probe.
///
/// Inputs:
/// - Native command execution results.
///
/// Outputs:
/// - Small target-neutral value set used by bridge tests.
///
/// Transformation:
/// - Avoids exposing Rust resources directly while still proving command/reply
///   routing and typed return values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeValue {
    Unit,
    Int(i64),
    Handle(NativeHandle),
}

/// Worker reply carrying request correlation and backpressure state.
///
/// Inputs:
/// - Request id supplied by the caller.
/// - Worker operation result.
/// - Remaining advertised credit window.
///
/// Outputs:
/// - Reply value the BEAM side can match against the original request.
///
/// Transformation:
/// - Makes request/reply correlation and credit-based flow control explicit in
///   the ABI-level probe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeReply {
    pub request_id: u64,
    pub result: Result<NativeValue, NativeBridgeError>,
    pub credits: usize,
}

/// Supervised native worker handle.
///
/// Inputs:
/// - Start requests from the BEAM supervision boundary.
///
/// Outputs:
/// - A command sender plus owned worker join handle.
///
/// Transformation:
/// - Owns native resource state on the Rust side and exposes only typed
///   request methods to callers.
pub struct SupervisedNativeWorker {
    tx: Sender<WorkerCommand>,
    join: Option<JoinHandle<()>>,
    credit_window: usize,
}

/// Commands accepted by the supervised native worker probe.
///
/// Inputs:
/// - BEAM-side request data, native handles, operation names, and reply
///   channels.
///
/// Outputs:
/// - Worker-loop actions that allocate, call, dispose, or stop native state.
///
/// Transformation:
/// - Serializes mutable native resource access into one Rust-owned command
///   stream.
enum WorkerCommand {
    AllocateDataFrame {
        request_id: u64,
        reply: Sender<NativeReply>,
    },
    Call {
        request_id: u64,
        handle: NativeHandle,
        operation: &'static str,
        reply: Sender<NativeReply>,
    },
    Dispose {
        request_id: u64,
        handle: NativeHandle,
        reply: Sender<NativeReply>,
    },
    Stop,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Native resource entry owned by the worker.
///
/// Inputs:
/// - Allocated native handle metadata.
///
/// Outputs:
/// - Resource-table value used to validate handles.
///
/// Transformation:
/// - Stores generation and type metadata so stale or forged handles can be
///   rejected before operations run.
struct Resource {
    generation: u64,
    type_name: &'static str,
}

impl SupervisedNativeWorker {
    /// Starts a supervised native worker probe.
    ///
    /// Inputs:
    /// - `credit_window`: maximum advertised outstanding request budget.
    ///
    /// Outputs:
    /// - Running worker handle.
    ///
    /// Transformation:
    /// - Spawns a Rust-owned actor loop that serializes mutable resource access.
    pub fn start(credit_window: usize) -> Self {
        let credit_window = credit_window.max(1);
        let (tx, rx) = mpsc::channel();
        let join = thread::spawn(move || worker_loop(rx, credit_window));

        Self {
            tx,
            join: Some(join),
            credit_window,
        }
    }

    /// Returns the worker credit window.
    ///
    /// Inputs:
    /// - `self`: running worker handle.
    ///
    /// Outputs:
    /// - Configured positive credit window.
    ///
    /// Transformation:
    /// - Exposes the backpressure budget recorded in worker replies.
    pub fn credit_window(&self) -> usize {
        self.credit_window
    }

    /// Allocates an opaque DataFrame handle.
    ///
    /// Inputs:
    /// - `request_id`: caller-supplied request correlation id.
    ///
    /// Outputs:
    /// - Reply containing `NativeValue::Handle` or a stable bridge error.
    ///
    /// Transformation:
    /// - Creates worker-owned resource state and returns only a typed handle.
    pub fn allocate_dataframe(&self, request_id: u64) -> NativeReply {
        let (reply, rx) = mpsc::channel();
        self.send_and_recv(
            WorkerCommand::AllocateDataFrame { request_id, reply },
            request_id,
            rx,
        )
    }

    /// Calls a read-only observer operation on a handle.
    ///
    /// Inputs:
    /// - `request_id`: caller-supplied request correlation id.
    /// - `handle`: opaque resource handle previously returned by the worker.
    /// - `operation`: selected operation name.
    ///
    /// Outputs:
    /// - Reply containing a typed result or stable bridge error.
    ///
    /// Transformation:
    /// - Routes operation execution through the resource owner actor and
    ///   validates handle generation before producing a result.
    pub fn call(
        &self,
        request_id: u64,
        handle: NativeHandle,
        operation: &'static str,
    ) -> NativeReply {
        let (reply, rx) = mpsc::channel();
        self.send_and_recv(
            WorkerCommand::Call {
                request_id,
                handle,
                operation,
                reply,
            },
            request_id,
            rx,
        )
    }

    /// Disposes a native resource handle.
    ///
    /// Inputs:
    /// - `request_id`: caller-supplied request correlation id.
    /// - `handle`: opaque resource handle to release.
    ///
    /// Outputs:
    /// - Reply containing `NativeValue::Unit` or a stale-handle error.
    ///
    /// Transformation:
    /// - Releases worker-owned state while preserving generation-token checks.
    pub fn dispose(&self, request_id: u64, handle: NativeHandle) -> NativeReply {
        let (reply, rx) = mpsc::channel();
        self.send_and_recv(
            WorkerCommand::Dispose {
                request_id,
                handle,
                reply,
            },
            request_id,
            rx,
        )
    }

    /// Stops the worker and joins its thread.
    ///
    /// Inputs:
    /// - `self`: owned worker handle.
    ///
    /// Outputs:
    /// - None.
    ///
    /// Transformation:
    /// - Sends an explicit stop command and waits for Rust-side cleanup.
    pub fn stop(mut self) {
        let _ = self.tx.send(WorkerCommand::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }

    /// Sends one command and waits for the correlated reply.
    ///
    /// Inputs:
    /// - `command`: worker command carrying its reply sender.
    /// - `request_id`: request id used if channel delivery fails.
    /// - `rx`: one-shot reply receiver.
    ///
    /// Outputs:
    /// - Worker reply or stable channel failure.
    ///
    /// Transformation:
    /// - Converts transport failures into bridge errors instead of panicking.
    fn send_and_recv(
        &self,
        command: WorkerCommand,
        request_id: u64,
        rx: Receiver<NativeReply>,
    ) -> NativeReply {
        if self.tx.send(command).is_err() {
            return NativeReply {
                request_id,
                result: Err(NativeBridgeError::new(
                    "native_worker_stopped",
                    "native worker is not accepting requests",
                )),
                credits: 0,
            };
        }

        rx.recv().unwrap_or_else(|_| NativeReply {
            request_id,
            result: Err(NativeBridgeError::new(
                "native_worker_stopped",
                "native worker stopped before replying",
            )),
            credits: 0,
        })
    }
}

impl Drop for SupervisedNativeWorker {
    /// Stops the worker when the handle is dropped.
    ///
    /// Inputs:
    /// - `self`: worker handle being dropped.
    ///
    /// Outputs:
    /// - None.
    ///
    /// Transformation:
    /// - Provides cleanup for tests and future BEAM resource finalizers that do
    ///   not call `stop` explicitly.
    fn drop(&mut self) {
        let _ = self.tx.send(WorkerCommand::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// Runs the resource-owner actor loop.
///
/// Inputs:
/// - `rx`: command receiver owned by the worker thread.
/// - `credit_window`: advertised request credit budget.
///
/// Outputs:
/// - None.
///
/// Transformation:
/// - Serializes all mutable resource operations and returns typed replies over
///   command-local reply channels.
fn worker_loop(rx: Receiver<WorkerCommand>, credit_window: usize) {
    let mut next_id = 1_u64;
    let mut resources = HashMap::<u64, Resource>::new();

    while let Ok(command) = rx.recv() {
        match command {
            WorkerCommand::AllocateDataFrame { request_id, reply } => {
                let id = next_id;
                next_id += 1;
                let handle = NativeHandle {
                    id,
                    generation: 1,
                    type_name: DATAFRAME_TYPE,
                };
                resources.insert(
                    id,
                    Resource {
                        generation: handle.generation,
                        type_name: handle.type_name,
                    },
                );
                let _ = reply.send(NativeReply {
                    request_id,
                    result: Ok(NativeValue::Handle(handle)),
                    credits: credit_window,
                });
            }
            WorkerCommand::Call {
                request_id,
                handle,
                operation,
                reply,
            } => {
                let result = match validate_handle(&resources, &handle) {
                    Ok(()) => call_operation(operation),
                    Err(err) => Err(err),
                };
                let _ = reply.send(NativeReply {
                    request_id,
                    result,
                    credits: credit_window,
                });
            }
            WorkerCommand::Dispose {
                request_id,
                handle,
                reply,
            } => {
                let result = match validate_handle(&resources, &handle) {
                    Ok(()) => {
                        resources.remove(&handle.id);
                        Ok(NativeValue::Unit)
                    }
                    Err(err) => Err(err),
                };
                let _ = reply.send(NativeReply {
                    request_id,
                    result,
                    credits: credit_window,
                });
            }
            WorkerCommand::Stop => break,
        }
    }
}

/// Validates an opaque handle against worker-owned resources.
///
/// Inputs:
/// - `resources`: current worker resource table.
/// - `handle`: caller-provided opaque handle.
///
/// Outputs:
/// - `Ok(())` when id, generation, and type match.
/// - Stable stale-handle error otherwise.
///
/// Transformation:
/// - Rejects stale or forged handles before any native operation executes.
fn validate_handle(
    resources: &HashMap<u64, Resource>,
    handle: &NativeHandle,
) -> Result<(), NativeBridgeError> {
    match resources.get(&handle.id) {
        Some(resource)
            if resource.generation == handle.generation
                && resource.type_name == handle.type_name =>
        {
            Ok(())
        }
        _ => Err(NativeBridgeError::new(
            "stale_native_handle",
            format!(
                "native handle {} generation {} is not live",
                handle.id, handle.generation
            ),
        )),
    }
}

/// Executes a small observer operation for the probe.
///
/// Inputs:
/// - `operation`: requested method name.
///
/// Outputs:
/// - Typed value for known operations or a stable unsupported-operation error.
///
/// Transformation:
/// - Keeps the P0.4a worker independent from real Polars while proving typed
///   native calls can route through the actor bridge.
fn call_operation(operation: &str) -> Result<NativeValue, NativeBridgeError> {
    match operation {
        "height" | "width" => Ok(NativeValue::Int(0)),
        other => Err(NativeBridgeError::new(
            "unsupported_native_operation",
            format!("native operation `{other}` is not implemented by this probe"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extracts a handle from an allocation reply.
    ///
    /// Inputs:
    /// - `reply`: worker allocation reply.
    ///
    /// Outputs:
    /// - Native handle stored in the reply.
    ///
    /// Transformation:
    /// - Panics only in tests if the worker contract returns the wrong value.
    fn handle_from(reply: NativeReply) -> NativeHandle {
        match reply.result.expect("allocation should succeed") {
            NativeValue::Handle(handle) => handle,
            other => panic!("expected handle, got {other:?}"),
        }
    }

    /// Verifies start/call/stop lifecycle with request ids and credits.
    ///
    /// Inputs:
    /// - Worker probe with a small credit window.
    ///
    /// Outputs:
    /// - Assertions over handle creation, observer call, request correlation,
    ///   and advertised credits.
    ///
    /// Transformation:
    /// - Exercises the BEAM-supervised actor bridge shape without native
    ///   package linkage.
    #[test]
    fn worker_allocates_handle_and_routes_typed_call() {
        let worker = SupervisedNativeWorker::start(4);
        let handle = handle_from(worker.allocate_dataframe(10));
        let reply = worker.call(11, handle, "height");

        assert_eq!(reply.request_id, 11);
        assert_eq!(reply.credits, 4);
        assert_eq!(reply.result, Ok(NativeValue::Int(0)));

        worker.stop();
    }

    /// Verifies disposed handles cannot be reused.
    ///
    /// Inputs:
    /// - Worker probe and one allocated handle.
    ///
    /// Outputs:
    /// - Assertions over successful disposal and stale-handle rejection.
    ///
    /// Transformation:
    /// - Proves generation-token based stale handle detection at the bridge
    ///   boundary.
    #[test]
    fn disposed_handle_is_rejected_as_stale() {
        let worker = SupervisedNativeWorker::start(DEFAULT_CREDIT_WINDOW);
        let handle = handle_from(worker.allocate_dataframe(20));

        assert_eq!(
            worker.dispose(21, handle.clone()).result,
            Ok(NativeValue::Unit)
        );

        let reply = worker.call(22, handle, "height");
        let err = reply.result.expect_err("disposed handle should fail");

        assert_eq!(err.code, "stale_native_handle");

        worker.stop();
    }

    /// Verifies unknown native operations fail with a stable error code.
    ///
    /// Inputs:
    /// - Worker probe and one allocated handle.
    ///
    /// Outputs:
    /// - Assertion over unsupported operation diagnostic.
    ///
    /// Transformation:
    /// - Ensures bridge errors are explicit before real adapter operations are
    ///   implemented.
    #[test]
    fn unsupported_operation_returns_stable_error() {
        let worker = SupervisedNativeWorker::start(DEFAULT_CREDIT_WINDOW);
        let handle = handle_from(worker.allocate_dataframe(30));
        let reply = worker.call(31, handle, "select");
        let err = reply
            .result
            .expect_err("select is not implemented by probe");

        assert_eq!(err.code, "unsupported_native_operation");

        worker.stop();
    }
}
"#;

pub(super) const POLARS_FILES: &[GeneratedFile] = &[
    GeneratedFile {
        path: "terlan.toml",
        contents: POLARS_TOML,
    },
    GeneratedFile {
        path: "src/std/native/polars/DataFrame.terl",
        contents: POLARS_DATAFRAME_TN,
    },
    GeneratedFile {
        path: "bindings/polars.mapping.toml",
        contents: POLARS_MAPPING_TOML,
    },
    GeneratedFile {
        path: "native/terlan-native.toml",
        contents: POLARS_NATIVE_ABI_TOML,
    },
    GeneratedFile {
        path: "docs/std.native.polars.md",
        contents: POLARS_PACKAGE_DOC,
    },
    GeneratedFile {
        path: "examples/read_csv.terl",
        contents: POLARS_READ_CSV_EXAMPLE,
    },
    GeneratedFile {
        path: "summaries/std.native.polars.DataFrame.typi",
        contents: POLARS_DATAFRAME_TYPI,
    },
    GeneratedFile {
        path: "native/rust/Cargo.toml",
        contents: POLARS_RUST_CARGO_TOML,
    },
    GeneratedFile {
        path: "native/rust/src/lib.rs",
        contents: POLARS_RUST_STUB,
    },
    GeneratedFile {
        path: "native/rust/src/bridge.rs",
        contents: POLARS_RUST_BRIDGE,
    },
];
