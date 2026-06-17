-module(std_core_error).

-moduledoc "Core typed error value.\n\n`std.core.Error` is the portable base error shape used by standard-library\nAPIs that return typed recoverable failures. Domain errors derive from this\nstruct and add context fields while preserving the common `code` and\n`message` fields.".

-export([code/1, message/1, new/2, to_string/1]).

-export_type([error/0]).

-record(error, {code, message}).

-type error() :: #error{}.

-doc "Creates an `Error`.\n\nInput: one symbolic error code and one human-readable message.\nOutput: an `Error` value with both fields populated.\nTransformation: constructs the public base error struct without relying on a\ntarget-specific exception or atom representation.\n\n```terlan\nmodule error_new_example.\n\nimport std.core.Error.{new}.\n\npub type Invalid = Atom[\"invalid\"].\n\npub demo(): Error ->\n   new(Invalid, \"invalid input\").\n```".

-spec new(atom(), binary()) -> error().

new(Code, Message) ->
    #error{code = Code, message = Message}.

-doc "Returns an error's symbolic code.\n\nInput: one `Error` receiver.\nOutput: the receiver's machine-readable `Atom` code.\nTransformation: reads the `code` field without changing the error value.".

-spec code(error()) -> atom().

code(Error) ->
    Error#error.code.

-doc "Returns an error's human-readable message.\n\nInput: one `Error` receiver.\nOutput: the receiver's `String` message.\nTransformation: reads the `message` field without changing the error value.".

-spec message(error()) -> binary().

message(Error) ->
    Error#error.message.

-doc "Converts an error to display text.\n\nInput: one `Error` receiver.\nOutput: the receiver's human-readable message.\nTransformation: delegates to `message()` so generic display paths can use the\nsame public representation.".

-spec to_string(error()) -> binary().

to_string(Error) ->
    message(Error).

