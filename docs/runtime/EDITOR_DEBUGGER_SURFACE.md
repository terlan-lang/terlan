# Terlan Editor Debugger Surface

Status: 0.0.7 contract gate.

This document defines the editor-facing debugger contract for the Terlan VM
runtime. It is a contract for editor packages, native-image metadata, and future
debug-adapter work. It does not require editor integrations to invent their own
runtime protocol.

## Debug Type

Editors must use the debug type `terlan-vm`.

The default launch path is:

```text
terlan-vm debug-adapter --stdio
```

The adapter command is the future transport for launch and attach sessions.
Editors must not depend on a separate language-server binary for debugging;
language features continue to use `terlc lsp --stdio`.

## Command Fallback

Until the debug adapter can stop and inspect live VM programs, editors and CI
tools use the native-image command-line debugger surface:

```text
terlc debug <image.tvm> [--break <module.function|file:line>] [--script <file.terldbg>] [--json-events]
```

The command admits a compiler-generated image and reports its descriptor,
exports, continuations, source records, and resolved breakpoints.
Machine-readable callers may combine it with:

```text
terlc --diagnostic-format json debug build/app.tvm --script session.terldbg
```

The command surface lets editor integrations validate launch configuration,
breakpoint resolution, script-path handling, and JSON event wiring against an
actually admitted image without pretending live stepping exists.

## Session Modes

The debugger surface must support:

- `launch`: compile a `.terl` source file or start a native `.tvm` image.
- `attach`: connect to an already running Terlan VM process.

Launch and attach configurations must carry enough information to identify:

- source or native-image path
- working directory
- entry function
- environment variables
- runtime arguments
- project manifest path when present

## DAP Operations

The editor-facing protocol must cover these Debug Adapter Protocol operations:

- `initialize`
- `launch`
- `attach`
- `disconnect`
- `setBreakpoints`
- `setFunctionBreakpoints`
- `configurationDone`
- `continue`
- `next`
- `stepIn`
- `stepOut`
- `pause`
- `threads`
- `stackTrace`
- `scopes`
- `variables`
- `evaluate`

## VM Inspection Scopes

Debugger variables must expose Terlan-owned runtime state, not backend internals:

- stack frames
- local variables
- function arguments
- process ids
- generation ids
- mailbox state
- NativeBoundary resources
- timers
- supervisors
- trace ids

Resource inspection must show typed handles and ownership metadata, not raw
pointers, foreign runtime environments, or backend scheduler terms.

## Native Image Debug Metadata

Native TVM images or their detached debug companions must include metadata sufficient for source-level
diagnostics and stepping:

- source map
- function map
- expression spans
- process ids
- generation ids
- NativeBoundary call spans
- trace ids

Native-image admission must reject missing source-map/debug metadata for
user-facing functions. Runtime diagnostics should prefer Terlan source spans
over native-image internals whenever metadata exists.

## Gate

The contract is guarded by:

```bash
make editor-debugger-surface-check
```
