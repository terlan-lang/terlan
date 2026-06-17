-module(std_encoding_base64).

-moduledoc "Portable Base64 encoding contract.\n\n`std.encoding.Base64` exposes text-oriented Base64 helpers through a\ntarget-neutral Terlan API. Its first backend is Rust/SafeNative with the\n`base64` crate; source code sees only `String`, `Base64Error`, and `Result`.".

-export([decode/1, decode_url/1, encode/1, encode_url/1]).

-export_type([base64_error/0]).

-record(base64error, {code, message, offset}).

-type base64_error() :: #base64error{}.

-doc "Encodes UTF-8 text with the standard Base64 alphabet.\n\nInput: one UTF-8 `String`.\nOutput: Base64 text using the standard alphabet with padding.\nTransformation: delegates encoding to the selected target Base64 capability.".

-spec encode(binary()) -> binary().

encode(Text) ->
    Native.

-doc "Decodes standard Base64 text into UTF-8 text.\n\nInput: one Base64 `String`.\nOutput: `Ok(text)` when decoding succeeds and the result is valid UTF-8,\notherwise `Err(Base64Error)`.\nTransformation: delegates decoding to the selected target Base64 capability\nand maps invalid input into `Base64Error`.".

-spec decode(binary()) -> std_core_result:result(binary(), base64_error()).

decode(Text) ->
    Native.

-doc "Encodes UTF-8 text with the URL-safe Base64 alphabet.\n\nInput: one UTF-8 `String`.\nOutput: Base64 text using the URL-safe alphabet without relying on a\nbackend-specific encoder surface.\nTransformation: delegates URL-safe encoding to the selected target Base64\ncapability.".

-spec encode_url(binary()) -> binary().

encode_url(Text) ->
    Native.

-doc "Decodes URL-safe Base64 text into UTF-8 text.\n\nInput: one URL-safe Base64 `String`.\nOutput: `Ok(text)` when decoding succeeds and the result is valid UTF-8,\notherwise `Err(Base64Error)`.\nTransformation: delegates URL-safe decoding to the selected target Base64\ncapability and maps invalid input into `Base64Error`.".

-spec decode_url(binary()) -> std_core_result:result(binary(), base64_error()).

decode_url(Text) ->
    Native.

