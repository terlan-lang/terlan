use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    run_vm_artifact_format, validate_native_data_abi_text, validate_vm_artifact_format_text,
};

/// Minimal complete TVM executable-image contract fixture.
const VALID_CONTRACT: &str = r#"
# TVM Executable Image Specification

Status: normative format-1 contract.

The primary 0.0.7 architectural decision is direct AOT compilation.

The target-specific TVM executable image is an ahead-of-time compiled program
containing native machine code and an embedded descriptor for the runtime ABI.
It is not Terlan bytecode, not a binary encoding of Terlan instructions, not
serialized CoreIR, not JSON, and not a JIT input. The VM is a runtime kernel and
supervisor, not a Terlan instruction interpreter.

ELF executable image
Mach-O executable image
PE executable image
format-1 canonical little-endian binary encoding
ASCII magic TVMDSC01
32-byte SHA-256 digest
.note.terlan.tvm
__tvm_desc
.tvm$D
records are strictly ordered by kind
boundary tags include 7 NativeResource

## Compiler Metadata Is Separate

Same-shard actor operations invoke typed runtime primitives directly.
NativeBoundary isolates unsafe package calls. Format 1 uses a supervised
execution-shard OS process and a failure must not crash or corrupt the
supervisor control plane.

## Loader Admission

## Determinism and Relocation

Builds use content-addressed internal caches, skip a true no-op build, and use
one final application-image link. Go-class compilation speed uses a
compiler-owned native-object backend and equivalent Terlan and Go reference
projects. The REPL remains AOT-only. Deployment emits one `.tvm` image. The warm
loop completes in less than one second.

Cranelift is the sole 0.0.7 application code-generation backend. Checked CoreIR
passes through Terlan-owned NativeIR. The compiler uses `cranelift-object`
in-process. Finalized code locations produce compact Terlan-owned stack maps.

The standalone runtime never falls back to serialized JSON or VMIR. Source
commands require native images and fail closed when managed AOT support is not
available. Conformance runs a Terlan consumer end to end.
"#;

/// Minimal complete native data ABI fixture for the combined release gate.
const VALID_NATIVE_ABI: &str = r#"
# TVM Native Data ABI Specification

Status: normative semantic contract.

Terlan is a state-of-the-art reimplementation and optimization of the Erlang
execution model. Every Erlang-derived test already ported to Terlan MUST retain
its fixture and assertions. The JavaScript backend is not part of the native
TVM execution pivot.

There is no universal native representation. The compiled value ABI, TVM
transport encoding, managed layout profile, and external NativeBoundary ABI
remain separate. An execution shard is an OS process. Only the scheduler thread
holding the actor's mutator token may execute it.

ABI 1 uses a 64-bit little-endian model and does not impose an eight-byte maximum
alignment. Int is fixed `i64`. Types have a 128-bit semantic type identity and
256-bit native layout fingerprint. Bool is exactly `0` or `1`. NaN and infinity
are rejected. Atoms are finite compiler-known identities. TvmRef is a
relocatable, actor-local managed reference.

The heap has a bump-pointer young-allocation fast path, actor-local collection
without stopping unrelated actors, precise root discovery, and relocation of all
live `TvmRef` values. The managed profile does not expose a public heap header.
Ordinary objects must not carry atomic strong and weak reference counts. Large
values use shared immutable storage. Escape Analysis And Allocation Elision is
required. The compiler emits precise stack maps; conservative native-stack
scanning is non-conforming.

Union native layout MAY use optimized representations. Collection physical
representation remains runtime- and compiler-private. Same-shard send does not
serialize and uses a typed compiler/runtime transfer plan. A receiver must never
observe an actor-local reference into the sender's heap. A closure contains a
function identity, and an owned environment. `Dynamic` and `Term` are explicit
language forms for heterogeneous tagged values.

Logical identities are pointer-free and generation checked. Generic code rejects
uncontrolled specialization.

The `terlan-native-v2` convention requires that no native unwinding crosses a
call. The common scheduler poll SHOULD be an inline fast path. Local runtime
operations must not require canonical serialization. TVM transport is canonical
data; it is not bytecode. Decoding is atomic.

## External NativeBoundary ABI

Unsafe adapters run outside the execution shard by default.

Native memory is never reinterpreted during hot reload.

## Rejection Requirements

Conformance compares a supported Erlang/OTP build on identical hardware and
proves no secondary application compiler is invoked.

Cranelift is the only conforming native code-generation backend. Checked CoreIR
lowers through Terlan NativeIR. Cranelift does not define the Terlan language
ABI. The compiler emits a compact, versioned Terlan stack-map section.
"#;

/// Temporary repository fixture for VM artifact checks.
///
/// Inputs:
/// - Created with a unique path under the system temporary directory.
///
/// Output:
/// - Fixture root path and automatic cleanup on drop.
///
/// Transformation:
/// - Provides a tiny repo-shaped directory without external test dependencies.
struct TestRepo {
    root: PathBuf,
}

impl TestRepo {
    /// Creates an empty repository fixture.
    ///
    /// Inputs:
    /// - `name`: diagnostic name segment for the temporary directory.
    ///
    /// Output:
    /// - New fixture root.
    ///
    /// Transformation:
    /// - Combines process id and time into a unique temp path, then creates
    ///   the root directory.
    fn new(name: &str) -> io::Result<Self> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "terlan-quality-vm-artifact-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Returns the fixture root path.
    ///
    /// Inputs:
    /// - The fixture.
    ///
    /// Output:
    /// - Borrowed repository root path.
    ///
    /// Transformation:
    /// - Exposes the root as a `Path` for quality-check execution.
    fn root(&self) -> &Path {
        &self.root
    }

    /// Writes the TVM executable-image contract fixture file.
    ///
    /// Inputs:
    /// - `text`: contract content.
    ///
    /// Output:
    /// - `Ok(())` when the file is written.
    ///
    /// Transformation:
    /// - Creates the runtime docs directory and writes UTF-8 text.
    fn write_contract(&self, text: &str) -> io::Result<()> {
        self.write_image_contract(text)?;
        let path = self.root.join("docs/runtime/TVM_NATIVE_DATA_ABI_SPEC.md");
        fs::write(path, VALID_NATIVE_ABI)
    }

    /// Writes only the executable-image contract for missing-ABI tests.
    fn write_image_contract(&self, text: &str) -> io::Result<()> {
        let path = self.root.join("docs/runtime/TVM_EXECUTABLE_IMAGE_SPEC.md");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, text)
    }
}

impl Drop for TestRepo {
    /// Removes the temporary repository fixture.
    ///
    /// Inputs:
    /// - The fixture root path.
    ///
    /// Output:
    /// - Best-effort cleanup.
    ///
    /// Transformation:
    /// - Deletes the temporary directory and ignores cleanup failures.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Verifies the contract check accepts a complete minimal artifact spec.
///
/// Inputs:
/// - Repository fixture containing all required semantic groups.
///
/// Output:
/// - Successful summary with enforced requirement count.
///
/// Transformation:
/// - Runs the same public checker used by Make and CI.
#[test]
fn vm_artifact_format_accepts_complete_executable_image_contract() {
    let repo = TestRepo::new("complete").expect("create fixture");
    repo.write_contract(VALID_CONTRACT)
        .expect("write contract fixture");

    let summary = run_vm_artifact_format(repo.root()).expect("contract should pass");

    assert!(summary.required_group_count > 10);
}

/// Verifies missing contract files fail with a stable diagnostic.
///
/// Inputs:
/// - Empty repository fixture.
///
/// Output:
/// - Error mentioning the missing TVM executable-image contract.
///
/// Transformation:
/// - Exercises the public file-loading path instead of only text validation.
#[test]
fn vm_artifact_format_rejects_missing_contract_file() {
    let repo = TestRepo::new("missing").expect("create fixture");

    let error = run_vm_artifact_format(repo.root()).expect_err("contract should be missing");

    assert!(error.contains("failed to read TVM executable-image contract"));
}

/// Verifies the combined gate fails when the native data ABI spec is absent.
#[test]
fn vm_artifact_format_rejects_missing_native_data_abi_contract() {
    let repo = TestRepo::new("missing-native-abi").expect("create fixture");
    repo.write_image_contract(VALID_CONTRACT)
        .expect("write image contract fixture");

    let error = run_vm_artifact_format(repo.root()).expect_err("native ABI should be missing");

    assert!(error.contains("failed to read TVM native data ABI contract"));
}

/// Verifies the checker reports missing required contract language.
///
/// Inputs:
/// - Contract text missing the native boundary requirement.
///
/// Output:
/// - Diagnostic for the missing native boundary requirement.
///
/// Transformation:
/// - Confirms individual semantic groups produce actionable messages.
#[test]
fn vm_artifact_format_rejects_missing_required_group() {
    let text = VALID_CONTRACT.replace("NativeBoundary isolates", "The package boundary isolates");

    let diagnostics = validate_vm_artifact_format_text(&text);

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.contains("NativeBoundary")));
}

/// Verifies the checker blocks bytecode as the TVM executable image.
///
/// Inputs:
/// - Otherwise valid contract with a forbidden default-runtime claim.
///
/// Output:
/// - Diagnostic naming the forbidden claim.
///
/// Transformation:
/// - Prevents accidental drift toward a bytecode execution image.
#[test]
fn vm_artifact_format_rejects_bytecode_default_claims() {
    let text = format!("{VALID_CONTRACT}\nTVM bytecode is the default executable image.\n");

    let diagnostics = validate_vm_artifact_format_text(&text);

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.contains("forbidden default-runtime claim")));
}

/// Verifies the discarded fixed atomic-reference-counted header cannot return
/// as a public native ABI requirement.
#[test]
fn vm_native_data_abi_rejects_fixed_atomic_heap_header_claim() {
    let text = format!("{VALID_NATIVE_ABI}\nThe ABI 1 heap header is 32 bytes.\n");

    let diagnostics = validate_native_data_abi_text(&text);

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.contains("forbidden native-ABI claim")));
}
