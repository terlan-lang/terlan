//! Runtime-capability lowering for CoreIR Erlang emission.
//!
//! Inputs:
//! - Lowered Erlang argument expressions for CoreIR runtime capabilities.
//!
//! Outputs:
//! - Erlang expressions that call the BEAM runtime behind Terlan `std.io`
//!   surfaces.
//!
//! Transformations:
//! - Maps portable runtime capability ids to BEAM filesystem and console
//!   operations while preserving Terlan value shapes such as `Unit` and
//!   `Result`.

use super::super::erl::ErlExpr;
use super::super::util::map_struct_name;
use super::{erl_remote_call, exact_args};

/// Lowers `runtime.console.println` to BEAM console output.
///
/// Inputs:
/// - `args`: one lowered Erlang text expression.
///
/// Output:
/// - `Some(begin io:format("~ts~n", [Text]), unit end)` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Emits the target-owned BEAM `io:format/2` call behind the portable
///   `std.io.Console.println` API and normalizes the source-level return value
///   to Terlan `Unit`.
pub(in crate::emit) fn lower_runtime_console_println(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [text] = exact_args(args, 1)?.try_into().ok()?;
    Some(ErlExpr::Raw(format!(
        "begin io:format(\"~ts~n\", [{}]), unit end",
        text.render()
    )))
}

/// Lowers `runtime.file.exists` to a BEAM regular-file check.
///
/// Inputs:
/// - `args`: one lowered Erlang path expression.
///
/// Output:
/// - `Some(filelib:is_regular(Path))` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Emits a target-owned BEAM filesystem query behind the portable
///   `std.io.File.exists` API and returns Terlan's boolean representation.
pub(in crate::emit) fn lower_runtime_file_exists(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [path] = exact_args(args, 1)?.try_into().ok()?;
    Some(erl_remote_call("filelib", "is_regular", vec![path]))
}

/// Lowers `runtime.file.read_text` to BEAM file reading.
///
/// Inputs:
/// - `args`: one lowered Erlang path expression.
///
/// Output:
/// - `Some(case file:read_file(Path) of ... end)` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Reads bytes through BEAM, decodes successful values as UTF-8 text, and
///   maps backend filesystem reasons into neutral `std.io.File.FileError`
///   records before returning the `Result[String, FileError]` shape.
pub(in crate::emit) fn lower_runtime_file_read_text(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [path] = exact_args(args, 1)?.try_into().ok()?;
    let file_error = map_struct_name("FileError");
    Some(ErlExpr::Raw(format!(
        "case file:read_file({}) of\n    {{ok, Bytes}} -> {{ok, unicode:characters_to_list(Bytes, utf8)}};\n    {{error, enoent}} -> {{error, #{}{{code = not_found, message = \"file not found\", path = {}}}}};\n    {{error, eacces}} -> {{error, #{}{{code = permission_denied, message = \"file permission denied\", path = {}}}}};\n    {{error, badarg}} -> {{error, #{}{{code = invalid_path, message = \"invalid file path\", path = {}}}}};\n    {{error, _}} -> {{error, #{}{{code = unknown, message = \"unknown file error\", path = {}}}}}\nend",
        path.render(),
        file_error,
        path.render(),
        file_error,
        path.render(),
        file_error,
        path.render(),
        file_error,
        path.render()
    )))
}

/// Lowers `runtime.file.write_text` to BEAM file writing.
///
/// Inputs:
/// - `args`: lowered Erlang path and text expressions.
///
/// Output:
/// - `Some(case file:write_file(Path, Text) of ... end)` when arity is two.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Writes text through BEAM and maps backend filesystem reasons into neutral
///   `std.io.File.FileError` records before returning `Result[Unit, FileError]`.
pub(in crate::emit) fn lower_runtime_file_write_text(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [path, text] = exact_args(args, 2)?.try_into().ok()?;
    let file_error = map_struct_name("FileError");
    Some(ErlExpr::Raw(format!(
        "case file:write_file({}, {}) of\n    ok -> {{ok, unit}};\n    {{error, enoent}} -> {{error, #{}{{code = not_found, message = \"file not found\", path = {}}}}};\n    {{error, eacces}} -> {{error, #{}{{code = permission_denied, message = \"file permission denied\", path = {}}}}};\n    {{error, badarg}} -> {{error, #{}{{code = invalid_path, message = \"invalid file path\", path = {}}}}};\n    {{error, _}} -> {{error, #{}{{code = unknown, message = \"unknown file error\", path = {}}}}}\nend",
        path.render(),
        text.render(),
        file_error,
        path.render(),
        file_error,
        path.render(),
        file_error,
        path.render(),
        file_error,
        path.render()
    )))
}

/// Lowers `runtime.file.append_text` to BEAM append-mode file writing.
///
/// Inputs:
/// - `args`: lowered Erlang path and text expressions.
///
/// Output:
/// - `Some(case file:write_file(Path, Text, [append]) of ... end)` when arity
///   is two.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Appends text through BEAM and maps backend filesystem reasons into neutral
///   `std.io.File.FileError` records before returning `Result[Unit, FileError]`.
pub(in crate::emit) fn lower_runtime_file_append_text(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [path, text] = exact_args(args, 2)?.try_into().ok()?;
    let file_error = map_struct_name("FileError");
    Some(ErlExpr::Raw(format!(
        "case file:write_file({}, {}, [append]) of\n    ok -> {{ok, unit}};\n    {{error, enoent}} -> {{error, #{}{{code = not_found, message = \"file not found\", path = {}}}}};\n    {{error, eacces}} -> {{error, #{}{{code = permission_denied, message = \"file permission denied\", path = {}}}}};\n    {{error, badarg}} -> {{error, #{}{{code = invalid_path, message = \"invalid file path\", path = {}}}}};\n    {{error, _}} -> {{error, #{}{{code = unknown, message = \"unknown file error\", path = {}}}}}\nend",
        path.render(),
        text.render(),
        file_error,
        path.render(),
        file_error,
        path.render(),
        file_error,
        path.render(),
        file_error,
        path.render()
    )))
}

/// Lowers `runtime.file.delete` to BEAM file deletion.
///
/// Inputs:
/// - `args`: one lowered Erlang path expression.
///
/// Output:
/// - `Some(case file:delete(Path) of ... end)` when arity is one.
/// - `None` for malformed arity.
///
/// Transformation:
/// - Deletes through BEAM and maps backend filesystem reasons into neutral
///   `std.io.File.FileError` records before returning `Result[Unit, FileError]`.
pub(in crate::emit) fn lower_runtime_file_delete(args: Vec<ErlExpr>) -> Option<ErlExpr> {
    let [path] = exact_args(args, 1)?.try_into().ok()?;
    let file_error = map_struct_name("FileError");
    Some(ErlExpr::Raw(format!(
        "case file:delete({}) of\n    ok -> {{ok, unit}};\n    {{error, enoent}} -> {{error, #{}{{code = not_found, message = \"file not found\", path = {}}}}};\n    {{error, eacces}} -> {{error, #{}{{code = permission_denied, message = \"file permission denied\", path = {}}}}};\n    {{error, badarg}} -> {{error, #{}{{code = invalid_path, message = \"invalid file path\", path = {}}}}};\n    {{error, _}} -> {{error, #{}{{code = unknown, message = \"unknown file error\", path = {}}}}}\nend",
        path.render(),
        file_error,
        path.render(),
        file_error,
        path.render(),
        file_error,
        path.render(),
        file_error,
        path.render()
    )))
}
