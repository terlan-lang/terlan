# std.binary

`std.binary` owns portable descriptor types for future binary and bitstring
construction, matching, and protocol layout support.

## Modules

- `std.binary.Binary`: inert segment descriptors such as `UInt[16]`,
  `IntBits[32]`, `Bytes[4]`, `Bits[3]`, and `Rest`, plus typed binary
  decode/construct error contracts and protocol shape metadata.

## Current Scope

The 0.0.7 baseline provides descriptor-directed protocol encoding and decoding.
It does not enable source-level binary pattern matching. VM-owned byte boundary
helpers validate protocol shapes and decode integer and fixed-byte fields from
prefixed frames.

`decode_exact`, `decode_prefix`, and `construct` currently return typed
`UnsupportedRuntime` errors so callers can depend on the final `Result` shape
without receiving raw host or VM failures.

Protocol shape metadata is executable today. `decode_fixed_header` validates a
shape and returns its encoded fixed header plus remaining VM-owned body bytes.
`ProtocolShapeSet` validates reusable direct layouts and exact aliases before
lookup. Aliases target direct shapes only; alias chains, missing targets,
duplicate names, and prefix-based matches are rejected deterministically.
`protocol_shape_set_decode`, `protocol_shape_set_encode_exact`, and
`protocol_shape_set_encode_prefix` resolve the selected direct or alias name
and delegate to the same checked descriptor codecs used without a registry.
`decode_prefixed_body` decodes signed and unsigned integer fields under the
shape's endian policy, slices fixed byte fields, returns exact-length VM-owned
`BitString` values for raw `Bits` fields, and preserves the terminal body.
`encode_exact` validates and packs all fixed fields plus a declared terminal
body. `encode_prefix` uses the same checked packing but emits only the fixed
header. Integer fields support signed/unsigned big/little-endian widths through
63 bits; fixed byte fields preserve immutable VM byte values. Raw `Bits`
fields accept exact-width `BitString` values of arbitrary length, pack adjacent
partial-byte fields without padding, and require the complete encoded result to
end on a byte boundary.

`split_header_body` validates an explicit byte boundary.
`split_protocol_header` derives that boundary from fixed `UInt`, `IntBits`,
`Bits`, and `Bytes` descriptors, stops before terminal `Rest`, and rejects
non-byte-aligned or truncated frames with typed errors.

## Checks

- `make binary-descriptor-check`: runs the descriptor std tests.
- `make binary-error-taxonomy-check`: runs the typed binary error surface tests.
- `make binary-protocol-helper-check`: runs protocol shape and staged helper
  tests.
- `make binary-protocol-benchmark-check`: executes correctness-checked fixed
  header and composed variable-body workloads and writes the explicitly scoped
  end-to-end benchmark report.
- `make binary-bitstring-processing-check`: umbrella gate for binary work; it
  currently delegates to descriptor, error taxonomy, and protocol helper checks.
