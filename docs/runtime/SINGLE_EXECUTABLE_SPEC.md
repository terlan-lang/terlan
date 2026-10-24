# Single-executable native applications

Native application projects (`[build] artifact = "terlan-vm"`, including the
default application artifact) produce `bin/<package>` or `bin/<package>.exe`.
This file contains the runtime, compiled application images, entry metadata,
service resources, and compiler-selected native helper artifacts. It is the
deployment unit. The compiler installation, source checkout, standard-library
source files, and build directory are not deployment requirements.

Library and raw module-image builds continue to emit `.tvm` images for explicit
loading and generation replacement. JavaScript and Wasm targets retain their
target-specific artifacts.

## Build and publication

Application images and HTTP prewarming data are generated under the private
`_build/.terlan/application` directory. Reusable AOT objects stay in the existing
compiler cache. The compiler copies its paired native runtime to a private file,
checks executable-format compatibility, appends the compiled resources, and
publishes the completed executable with a rename. A failed payload build leaves
the existing application executable intact. Incremental builds preserve an
unchanged executable.

The paired runtime must be a native ELF, PE, or Mach-O executable. Shell wrappers
cannot become application executables. The private runtime copy is writable
while payload data is emitted, including when the installed runtime is read-only.

The build report has `executable.embedded = true`. Its `path`, `runtime`, and
`native_worker` fields identify the same executable. The external `image` and
`web_root` fields are omitted. A service's `service_runtime` also identifies that
executable. Release artifact collection deduplicates these paths.

When rebuilding a recognized legacy bundle, the compiler removes the previous
application's recorded runtime files and entry image. Unknown files, other
programs, symlinked output trees, and unrelated module images are preserved.

## Resource encoding

The native runtime prefix is followed by resource bytes in descriptor order,
a UTF-8 JSON descriptor, and this 64-byte footer:

| Offset | Bytes | Meaning |
| --- | --- | --- |
| 0 | 16 | `TERLAN_APP_V1` followed by three zero bytes |
| 16 | 8 | Resource bytes plus descriptor length, unsigned little-endian |
| 24 | 8 | Descriptor length, unsigned little-endian |
| 32 | 32 | SHA-256 of resource bytes and descriptor |

The descriptor selects a CLI image and zero-argument entry function, or an HTTP
service resource root. Each resource records a canonical relative path, offset,
length, SHA-256 digest, and executable permission. Native artifact environment
bindings name embedded resources, replacing paths into the compiler's artifact
cache at startup.

The descriptor is bounded to 8 MiB and 65,536 resources. Offsets must be
contiguous and cover the exact resource region. Paths reject traversal, absolute
names, platform device filenames, and ambiguous separators. Duplicate resource
paths and environment names are rejected. Native bindings cannot replace the
internal worker-role selector. The descriptor contains resource metadata;
application instructions remain in the compiled native image.

## Startup and ownership

Startup opens the running executable, validates descriptor bounds and payload
digest, and materializes resources into an exclusively created private directory.
Resource digests are checked again during copying. The ordinary native-image
loader then inspects and seals the application image before execution.

CLI arguments are passed unchanged to the compiled entrypoint. Service startup
uses the embedded, prewarmed web root and persisted-image runtime. Existing
`PORT`, `TERLAN_SERVE_PORT`, and trusted-host capability configuration remain
available. Native artifact bindings are installed before application owners and
service threads start.

Resource ownership outlives all application calls and service owners. Normal
shutdown removes the private resource tree. Existing native-image admission and
supervised process teardown remain responsible for their own resources.

## Native workers

Workers execute the same application executable under the existing capability
sandbox and bounded protocol. The supervisor supplies the reserved internal
worker role. Worker startup enters the native worker directly and still verifies
the sandbox, environment, limits, and descriptors before processing requests.

On Linux, the runtime retains its admitted executable handle. The first worker
request copies its native runtime prefix into a private, read-only executable
with an internal worker descriptor. Application resources are not copied again.
Workers mount this snapshot through the existing sandbox, so renaming or
replacing the deployed path cannot redirect execution. All worker paths retain
their canonical-path checks. Application inputs and `PATH` do not select the
retained executable or grant worker authority.

## Validation

`crates/terlan/tests/support/single_executable.rs` compiles real Terlan programs, removes
their project and build inputs, relocates the executable, and runs CLI and HTTP
checks. Linux cases hide the compiler and standard library with Bubblewrap and
exercise embedded static assets. Runtime tests cover descriptor bounds, resource
paths, digest corruption, mutation during admission, and cleanup. Worker tests
exercise the real sandboxed SQLite protocol, durable checkpoint recovery, and
retained executable lifetime.
