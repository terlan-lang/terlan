# Terlan

### Write once. Compile everywhere.

Terlan is a statically typed, functional programming language for safe and
predictable software.

It is designed for industrial systems across VM-hosted, web, native, and future
embedded targets.

The 0.0.8 release line uses the compiler-owned Rust `terlan-vm` as the default
runtime direction. Terlan keeps the supervision, actor, mailbox,
hot-reload, and fault-tolerance goals that made BEAM attractive, but the product
runtime is no longer the old OTP/BEAM execution path.

Terlan favors explicit types, immutable data, pattern matching, readable
functional pipelines, and target-specific standard libraries. Advanced features
are being added where they make production code clearer: table-driven tests,
property tests, shape guards, string and binary patterns, typed templates,
native bindings, and VM-owned HTTP/networking.

## Status

Current version: `0.0.8`.

Terlan is still early and experimental. The syntax, runtime, standard library,
VM, editor integrations, and release tooling are changing quickly. The current
goal is not maturity; the goal is to make the VM-default compiler path real,
tested, and predictable.

Current direction:

- `terlc` is the compiler and project tool.
- `terlan-vm` is the default runtime target under active hardening.
- OTP/BEAM references are legacy inventory, semantics comparison material, or
  migration checkpoints.
- JavaScript, Wasm, HTTP, native packages, editor tooling, and debugger support
  are active experimental surfaces.
- Standard-library APIs must have executable coverage, adversarial tests where
  appropriate, and documentation that can be served to tools.

## Install

Install the latest published platform artifact:

```sh
curl -fsSL https://raw.githubusercontent.com/terlan-lang/terlan/main/install.sh | sh
```

Pin a published release through the installer:

```sh
curl -fsSL https://raw.githubusercontent.com/terlan-lang/terlan/main/install.sh | env TERLAN_VERSION=v0.0.8 sh
```

On Windows, use PowerShell:

```powershell
iwr https://raw.githubusercontent.com/terlan-lang/terlan/main/install.ps1 -UseBasicParsing | iex
```

Install from a checkout with Rust:

```sh
cargo install --path crates/terlan --bin terlc --force
terlc --version
```

## Hello World

Create a project:

```sh
terlc init hello
cd hello
```

`terlc init` creates a runnable Terlan module:

```terlan
module hello.Main.

import std.io.Console.{println}.

pub main(): Unit ->
    println("hello from Terlan").
```

Run it:

```sh
terlc run
```

Expected output:

```text
hello from Terlan
```

## Function Head Migration

README quickstart migration example:

```text
Before:
pub full_name(user = {name, family_name}: User): String -> name + " " + family_name.

After:
pub full_name({name, family_name} = user: User): String -> name + " " + family_name.
```

The CLI diagnostic links this rewrite to
`docs/language/function_heads.md#migrationfunction_head_patterninvalid_alias_style`
and the stable ID `migration.function_head_pattern.invalid_alias_style`.

## String Pattern Captures

String patterns can bind delimited text directly. An inferred capture binds a
`String`:

```terlan
let "assets/${bucket}/${file}.txt" = path;
bucket + "/" + file.
```

A typed capture parses and checks the captured text before its clause runs:

```terlan
case request_line {
    "GET /users/${id: Int}" where id > 0 -> id;
    _ -> 0
}.
```

The same capture syntax works in `let`, `case`, function-head, and lambda
patterns. Guards use `where`. Adjacent captures without a literal delimiter are
rejected because their boundary would be ambiguous.

## Tests

`terlc init` also creates a sample test file:

```text
tests/hello/MainTest.terl
```

Run it with:

```sh
terlc test tests/hello/MainTest.terl
```

Expected output:

```text
running 1 tests
test hello_text_is_stable ... ok
test result: ok. 1 passed; 0 failed
```

The standard test library now includes assertion helpers, table-driven tests,
lifecycle hooks, property generators, shrinking/reporting support, and fake-test
detection. These are used by the standard library itself, not kept as examples
only.

## Runtime Model

Terlan 0.0.8 is centered on `terlan-vm`.

The VM owns the runtime semantics that should not leak into application code:
processes, mailboxes, scheduling, resources, VM-owned collections, native
capability boundaries, HTTP transport ownership, and future debugger and
hot-reload behavior.

Native capabilities are expected to cross a compiler and VM-owned boundary. They
must not depend on ad hoc runtime escape hatches. Where maintained Rust crates
exist for parsing, TLS, SQL, Wasm, or protocol work, Terlan should reuse them
instead of hand-rolling fragile infrastructure.

## Targets

The active targets are:

- `vm`: the default Terlan VM target.
- `js.shared`: library-style JavaScript modules.
- `js.browser`: browser APIs plus packaged web assets.
- `js.worker`: worker-safe JavaScript APIs.
- `wasm.core`: first Wasm core artifact path for explicit ABI types.

Target selection should become increasingly type-driven. For example, importing
`std.wasm.Abi.I32` should be enough for the compiler to infer a Wasm core
artifact when the module shape requires it.

## JavaScript And Web

Terlan can emit library-style JavaScript modules:

```sh
terlc build --target js.shared
```

It can also package a browser web artifact:

```sh
terlc init hello-web --profile web
cd hello-web
terlc build --target js.browser
terlc serve
```

The JavaScript standard library is generated from TypeScript declaration
surfaces and lives under `std.js` and related browser namespaces. Missing
TypeScript standard-library or DOM declarations must be intentional and
justified, not accidental.

The JavaScript target validates emitted JavaScript with Oxc and rejects
target-only imports on incompatible targets.

## HTTP

Terlan HTTP is moving into the VM runtime instead of being a thin wrapper over
another framework. The long-term split is strict:

- maintained Rust crates own protocol parsing and cryptography;
- the Terlan VM owns streams, backpressure, cancellation, handler scheduling,
  session actors, hot reload, and fault-tolerance semantics.

`std.http` currently covers request, response, cookies, routing, sessions, TLS
configuration, and VM transport work. ACME/Let's Encrypt support remains part
of the later hardening track.

## Standard Library

The standard library includes core types, collections, object/map support,
JSON, random, regex, ranges, paths/files, logging, templates, testing, VM
primitives, HTTP descriptors, JavaScript bindings, and Wasm ABI types.

Standard-library APIs are expected to be documented and covered by executable
tests. Coverage gates are intentionally strict so release APIs do not silently
outgrow their tests.

## Editor Support

The repository includes editor support for syntax, icons, runnable tests, and
language-server work. VS Code is the most active integration, with Neovim,
Emacs, and IntelliJ tracked as supported editor surfaces.

Editor integrations should serve documentation for modules, structs, functions,
methods, and generated standard-library declarations.

## Development Checks

Useful focused checks:

```sh
make std-test-honesty-check
make std-test-table-check
make std-test-property-check
make std-package-coverage-100-check
make stdlib-release-tests-vm-default-check
make all-terlan-tests-vm-check
make terlan-vm-run-command-check
make vm-release-install-validation-check
make wasm-coreir-lowering-check
make vm-performance-baseline-check
make vm-coverage-100-check
make vm-coverage-source-lines-check
```

Broader local validation:

```sh
make check
```

Release correctness is defined by semantic validation aggregates and their
candidate-bound evidence. Roadmaps describe planning history and are not build
inputs.

### Publication

From a clean `main` commit with successful Compiler CI and Release validation:

```sh
make publish
```

The version comes from the workspace. Publication requires authenticated GitHub
CLI access and a Linux x86_64 environment capable of running the hosted artifact
(Ubuntu 24.04-compatible userspace), with Node 24 for installed JavaScript
examples. An older Linux host can use a compatible container; CPU quietness and
a self-hosted GitHub runner are not prerequisites.

Publication retains the verified hosted archives while refreshing stale local
evidence, then restores those exact bytes before sealing installed-candidate
reports. Successful candidate tests are not replayed during publication retries.

## Documentation

Important design documents:

- `docs/grammar/TERLAN_SYNTAX_SPEC.ebnf`
- `docs/compiler/TERLAN_CORE_TYPING_SPEC.md`
- `docs/compiler/TERLAN_TARGET_INFERENCE.md`
- `docs/runtime/TVM_EXECUTABLE_IMAGE_SPEC.md`
- `docs/runtime/TVM_NATIVE_DATA_ABI_SPEC.md`
- `docs/runtime/TERLAN_VM_OWNERSHIP.md`
- `docs/runtime/TERLAN_VM_RUNTIME_CONCEPTS.md`
- `docs/runtime/OTP_RUNTIME_EXIT.md`
- `docs/runtime/EDITOR_DEBUGGER_SURFACE.md`

Lean and formal-spec work are part of the long-term quality track. They are not
required for ordinary end-user compilation, but compiler behavior should keep
moving toward a documented, checkable type and runtime model.
