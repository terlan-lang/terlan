/**
 * Portable filesystem access.
 *
 * `std.io.File` exposes target-neutral file operations. Backends own the
 * runtime calls needed to implement these operations and must map backend
 * errors into the neutral `FileError` values defined here.
 */

module std.io.File.

import std.core.Result.{Err}.
import type std.core.Result.Result.
import std.core.Unit.{Unit}.

/**
 * The file path did not resolve to an existing file.
 */
pub type NotFound = Atom["not_found"].

/**
 * The selected target denied access to the file.
 */
pub type PermissionDenied = Atom["permission_denied"].

/**
 * The supplied path cannot be represented by the selected target.
 */
pub type InvalidPath = Atom["invalid_path"].

/**
 * The selected target reported an unclassified filesystem failure.
 */
pub type Unknown = Atom["unknown"].

/**
 * Portable file error values returned by `std.io.File`.
 */
pub type FileError =
      NotFound
    | PermissionDenied
    | InvalidPath
    | Unknown.

/**
 * Returns whether a file exists at `path`.
 *
 * Input: one UTF-8 path string interpreted by the selected target runtime.
 * Output: `true` when a regular file exists, otherwise `false`.
 * Transformation: lowers through the compiler-owned `runtime.file.exists`
 * capability and keeps backend path checks behind the portable API.
 */
@compiler.intrinsic {runtime.file.exists}
pub exists(path: String): Bool ->
    false.

/**
 * Reads a UTF-8 text file.
 *
 * Input: one UTF-8 path string interpreted by the selected target runtime.
 * Output: `Result[String, FileError]`.
 * Transformation: lowers through the compiler-owned `runtime.file.read_text`
 * capability. Backend file errors are normalized into `FileError` values, and
 * successful file bytes are decoded as UTF-8 text for Terlan `String`.
 */
@compiler.intrinsic {runtime.file.read_text}
pub read_text(path: String): Result[String, FileError] ->
    Err(Unknown).

/**
 * Writes a UTF-8 text file.
 *
 * Input: a UTF-8 path string and UTF-8 text content.
 * Output: `Result[Unit, FileError]`.
 * Transformation: lowers through the compiler-owned `runtime.file.write_text`
 * capability. Backend file errors are normalized into `FileError` values, and a
 * successful write returns `Unit`.
 */
@compiler.intrinsic {runtime.file.write_text}
pub write_text(path: String, content: String): Result[Unit, FileError] ->
    Err(Unknown).
