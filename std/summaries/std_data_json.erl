-module(std_data_json).

-moduledoc "Portable JSON value contract.\n\n`std.data.Json` exposes JSON parsing, rendering, and typed accessors through\na target-neutral Terlan API. Its first backend is Rust/SafeNative with\n`serde_json`, but source code sees only `Json`, `JsonError`, and `Result`.".

-export([as_bool/1, as_float/1, as_int/1, as_string/1, get/2, is_null/1, parse/1, stringify/1]).

-export_type([json/0, json_error/0]).

-record(jsonerror, {code, message, offset}).

-doc "Json represents a parsed JSON value.\n\nInput: no type parameter.\nOutput: an opaque JSON handle whose runtime representation is target-owned.\nTransformation: hides the selected target JSON representation behind a\nstable portable API so application code does not depend on backend parser\ninternals.".

-opaque json() :: term().

-type json_error() :: #jsonerror{}.

-doc "Parses text into a JSON value.\n\nInput: one UTF-8 string containing JSON source text.\nOutput: `Ok(json)` when parsing succeeds, otherwise `Err(JsonError)`.\nTransformation: delegates parsing to the selected target JSON capability and\nmaps parser failures into `JsonError`.".

-spec parse(binary()) -> std_core_result:result(json(), json_error()).

parse(Text) ->
    Native.

-doc "Renders a JSON value as text.\n\nInput: one `Json` value.\nOutput: `Ok(text)` when rendering succeeds, otherwise `Err(JsonError)`.\nTransformation: delegates serialization to the selected target JSON\ncapability while preserving the portable `String` output contract.".

-spec stringify(json()) -> std_core_result:result(binary(), json_error()).

stringify(Json) ->
    Native.

-doc "Reads an object member by key.\n\nInput: one `Json` receiver and one UTF-8 object key.\nOutput: `Ok(value)` when the receiver is an object containing the key,\notherwise `Err(JsonError)`.\nTransformation: performs a typed JSON object lookup without exposing the\nbackend object representation.".

-spec get(json(), binary()) -> std_core_result:result(json(), json_error()).

get(Json, Key) ->
    Native.

-doc "Reads a JSON string value.\n\nInput: one `Json` receiver.\nOutput: `Ok(value)` when the receiver is a JSON string, otherwise\n`Err(JsonError)`.\nTransformation: validates the JSON value kind before converting it to the\nportable `String` type.".

-spec as_string(json()) -> std_core_result:result(binary(), json_error()).

as_string(Json) ->
    Native.

-doc "Reads a JSON integer value.\n\nInput: one `Json` receiver.\nOutput: `Ok(value)` when the receiver is a JSON integer representable as\n`Int`, otherwise `Err(JsonError)`.\nTransformation: validates the JSON value kind and numeric range before\nconverting it to the portable `Int` type.".

-spec as_int(json()) -> std_core_result:result(integer(), json_error()).

as_int(Json) ->
    Native.

-doc "Reads a JSON floating-point value.\n\nInput: one `Json` receiver.\nOutput: `Ok(value)` when the receiver is a JSON number representable as\n`Float`, otherwise `Err(JsonError)`.\nTransformation: validates the JSON value kind before converting it to the\nportable `Float` type.".

-spec as_float(json()) -> std_core_result:result(float(), json_error()).

as_float(Json) ->
    Native.

-doc "Reads a JSON boolean value.\n\nInput: one `Json` receiver.\nOutput: `Ok(value)` when the receiver is a JSON boolean, otherwise\n`Err(JsonError)`.\nTransformation: validates the JSON value kind before converting it to the\nportable `Bool` type.".

-spec as_bool(json()) -> std_core_result:result(boolean(), json_error()).

as_bool(Json) ->
    Native.

-doc "Returns whether a JSON value is null.\n\nInput: one `Json` receiver.\nOutput: `true` when the receiver is JSON null, otherwise `false`.\nTransformation: observes the JSON value kind without changing the receiver.".

-spec is_null(json()) -> boolean().

is_null(Json) ->
    Native.

