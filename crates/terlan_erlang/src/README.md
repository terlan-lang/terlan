# Terlan Erlang Source Internals

This directory owns the Erlang/BEAM backend crate source. It converts Terlan
syntax-output or transitional CoreIR bridge payloads into Erlang source,
runtime helpers, and struct header artifacts.

## Responsibilities

- Expose public Erlang emission entry points.
- Keep Erlang-specific rendering behind the backend crate.
- Preserve the CoreIR syntax-bridge validation during the transition away from
  direct syntax-output emission.
- Generate supporting Erlang artifacts such as runtime helpers and struct
  headers.

## Public Surface

- `try_emit_syntax_module_output_to_erlang`: direct syntax-output emission
  entry point.
- `try_emit_core_module_to_erlang_with_syntax_bridge`: CoreIR-gated
  transitional emission entry point.
- `try_emit_syntax_struct_headers_to_hrl`: struct header emission entry point.
- `emit_html_runtime_to_erlang`: HTML runtime helper emission.

## Core Model

The backend lowers compiler-owned intermediate structures into an Erlang module
model, then renders deterministic Erlang source. Erlang details stay here and
must not leak into syntax, HIR, type checking, or std source APIs.

The main flow is:

1. Receive syntax output or a validated CoreIR bridge payload.
2. Lower declarations, expressions, patterns, and runtime helpers into Erlang
   backend structures.
3. Render Erlang source or header text.
4. Return stable diagnostics when a source form is unsupported.

Important invariants:

- Backend emission is deterministic.
- CoreIR bridge emission validates schema, module identity, source kind, and
  syntax contract fingerprint.
- Erlang-specific names and tuples are backend implementation details unless a
  std module explicitly exposes a BEAM API.

## Integration Points

- `terlan_syntax`: supplies syntax-output payloads.
- `terlan_typeck`: supplies CoreIR payloads during formal backend handoff.
- `terlan_html`: supplies parsed templates and Markdown documents.
- `terlan_cli`: invokes the backend for build, test, and emit commands.

## Edge Cases

- Unsupported direct syntax emission must fail clearly instead of emitting
  partial Erlang.
- File imports and templates must be passed through artifact-aware backend
  entry points.
- BEAM handler response ABI remains internal to server bridge code.

## Types And Interfaces

`emit`
: Backend lowering and rendering module.

`pretty`
: Erlang source formatting support.

## Testing Notes

- Backend tests live beside emission modules as `*_test.rs` files.
- Runtime smoke tests should compile emitted Erlang with `erlc` when available.
- CoreIR bridge validation needs exact tests for stale or mismatched payloads.
