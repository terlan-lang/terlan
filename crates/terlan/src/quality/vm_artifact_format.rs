use std::fs;
use std::path::Path;

use crate::terlan_quality::QualityResult;

/// Repository-relative location of the normative TVM executable-image contract.
const VM_ARTIFACT_FORMAT_DOC: &str = "docs/runtime/TVM_EXECUTABLE_IMAGE_SPEC.md";

/// Repository-relative location of the normative TVM native data ABI contract.
const VM_NATIVE_DATA_ABI_DOC: &str = "docs/runtime/TVM_NATIVE_DATA_ABI_SPEC.md";

/// Required semantic groups for the VM artifact contract.
const REQUIRED_GROUPS: &[RequiredGroup] = &[
    RequiredGroup::single("title", "tvm executable image specification"),
    RequiredGroup::single("normative format-1 status", "normative format-1 contract"),
    RequiredGroup::single(
        "primary direct-AOT decision",
        "primary 0.0.7 architectural decision",
    ),
    RequiredGroup::single("target-specific image", "target-specific"),
    RequiredGroup::single("ahead-of-time compilation", "ahead-of-time compiled"),
    RequiredGroup::single("native machine code", "native machine code"),
    RequiredGroup::single("runtime ABI", "runtime abi"),
    RequiredGroup::single("not Terlan bytecode", "terlan bytecode"),
    RequiredGroup::single(
        "not encoded Terlan instructions",
        "binary encoding of terlan instructions",
    ),
    RequiredGroup::single("not serialized compiler IR", "serialized coreir"),
    RequiredGroup::single("not JSON", "json"),
    RequiredGroup::single("not JIT", "jit"),
    RequiredGroup::single("runtime-kernel model", "runtime kernel"),
    RequiredGroup::single(
        "non-interpreter model",
        "not a terlan instruction interpreter",
    ),
    RequiredGroup::single("ELF representation", "elf executable image"),
    RequiredGroup::single("Mach-O representation", "mach-o executable image"),
    RequiredGroup::single("PE representation", "pe executable image"),
    RequiredGroup::single("embedded descriptor", "embedded descriptor"),
    RequiredGroup::single(
        "canonical descriptor",
        "canonical little-endian binary encoding",
    ),
    RequiredGroup::single("descriptor magic", "tvmdsc01"),
    RequiredGroup::single("descriptor footer", "32-byte sha-256 digest"),
    RequiredGroup::single("ELF descriptor section", ".note.terlan.tvm"),
    RequiredGroup::single("Mach-O descriptor section", "__tvm_desc"),
    RequiredGroup::single("PE descriptor section", ".tvm$d"),
    RequiredGroup::single("ordered records", "strictly ordered by kind"),
    RequiredGroup::single("boundary type tags", "7 nativeresource"),
    RequiredGroup::single(
        "compiler-metadata separation",
        "compiler metadata is separate",
    ),
    RequiredGroup::single(
        "same-shard runtime fast path",
        "same-shard actor operations invoke typed runtime primitives directly",
    ),
    RequiredGroup::single("NativeBoundary", "nativeboundary"),
    RequiredGroup::single("shard isolation", "supervised execution-shard os process"),
    RequiredGroup::single(
        "crash isolation",
        "must not crash or corrupt the supervisor control plane",
    ),
    RequiredGroup::single("loader admission", "loader admission"),
    RequiredGroup::single("determinism", "determinism and relocation"),
    RequiredGroup::single(
        "content-addressed cache",
        "content-addressed internal caches",
    ),
    RequiredGroup::single("Go-class compilation", "go-class compilation speed"),
    RequiredGroup::single(
        "direct object backend",
        "compiler-owned native-object backend",
    ),
    RequiredGroup::single(
        "sole Cranelift backend",
        "cranelift is the sole 0.0.7 application code-generation backend",
    ),
    RequiredGroup::single("Terlan-owned NativeIR", "terlan-owned nativeir"),
    RequiredGroup::single(
        "in-process Cranelift object emission",
        "uses `cranelift-object` in-process",
    ),
    RequiredGroup::single(
        "Terlan-owned stack-map format",
        "compact terlan-owned stack maps",
    ),
    RequiredGroup::single(
        "Go comparison",
        "equivalent terlan and go reference projects",
    ),
    RequiredGroup::single("no-op build", "true no-op build"),
    RequiredGroup::single("single final link", "one final application-image link"),
    RequiredGroup::single("AOT REPL", "the repl remains aot-only"),
    RequiredGroup::single("sub-second warm loop", "less than one second"),
    RequiredGroup::single("single deployable image", "one `.tvm` image"),
    RequiredGroup::single(
        "no serialized runtime fallback",
        "never falls back to serialized json or vmir",
    ),
    RequiredGroup::single("end-to-end conformance", "terlan consumer end to end"),
];

/// Required semantic groups for the native value, call, and transition ABI.
const NATIVE_ABI_REQUIRED_GROUPS: &[RequiredGroup] = &[
    RequiredGroup::single("native ABI title", "tvm native data abi specification"),
    RequiredGroup::single("normative semantic status", "normative semantic contract"),
    RequiredGroup::single(
        "Erlang optimization objective",
        "state-of-the-art reimplementation and optimization of the erlang execution model",
    ),
    RequiredGroup::single(
        "ported-suite preservation",
        "every erlang-derived test already ported to terlan must retain",
    ),
    RequiredGroup::single(
        "JavaScript preservation",
        "javascript backend is not part of the native tvm execution pivot",
    ),
    RequiredGroup::single(
        "no universal native value",
        "no universal native representation",
    ),
    RequiredGroup::single("compiled value ABI", "compiled value abi"),
    RequiredGroup::single("managed layout profile", "managed layout profile"),
    RequiredGroup::single("transport encoding", "tvm transport encoding"),
    RequiredGroup::single("external adapter ABI", "external nativeboundary abi"),
    RequiredGroup::single("execution shard", "an execution shard is an os process"),
    RequiredGroup::single("actor mutator exclusion", "actor's mutator token"),
    RequiredGroup::single("64-bit target model", "64-bit little-endian"),
    RequiredGroup::single(
        "vector alignment",
        "does not impose an eight-byte maximum alignment",
    ),
    RequiredGroup::single("fixed Terlan Int", "fixed `i64`"),
    RequiredGroup::single("128-bit type identity", "128-bit semantic type identity"),
    RequiredGroup::single("layout fingerprint", "256-bit native layout fingerprint"),
    RequiredGroup::single("strict Bool", "exactly `0` or `1`"),
    RequiredGroup::single("finite Float", "nan and infinity are rejected"),
    RequiredGroup::single("finite atoms", "finite compiler-known identities"),
    RequiredGroup::single(
        "actor-local TvmRef",
        "relocatable, actor-local managed reference",
    ),
    RequiredGroup::single("bump allocation", "bump-pointer young-allocation fast path"),
    RequiredGroup::single(
        "actor-local collection",
        "actor-local collection without stopping unrelated actors",
    ),
    RequiredGroup::single("precise roots", "precise root discovery"),
    RequiredGroup::single("moving references", "relocation of all live `tvmref`"),
    RequiredGroup::single(
        "private heap header",
        "does not expose a public heap header",
    ),
    RequiredGroup::single(
        "no ordinary atomic ARC",
        "must not carry atomic strong and weak reference counts",
    ),
    RequiredGroup::single("shared immutable storage", "shared immutable storage"),
    RequiredGroup::single("escape analysis", "escape analysis and allocation elision"),
    RequiredGroup::single("precise stack maps", "compiler emits precise stack maps"),
    RequiredGroup::single(
        "no conservative scan",
        "conservative native-stack scanning is non-conforming",
    ),
    RequiredGroup::single("optimized unions", "native layout may use"),
    RequiredGroup::single(
        "private collections",
        "physical representation remains runtime- and compiler-private",
    ),
    RequiredGroup::single(
        "typed message transfer",
        "typed compiler/runtime transfer plan",
    ),
    RequiredGroup::single(
        "same-shard no serialization",
        "same-shard send does not serialize",
    ),
    RequiredGroup::single(
        "cross-actor safety",
        "must never observe an actor-local reference into the sender's heap",
    ),
    RequiredGroup::single(
        "closure identity",
        "function identity, and an owned environment",
    ),
    RequiredGroup::single(
        "restricted dynamic boxing",
        "dynamic` and `term` are explicit language forms",
    ),
    RequiredGroup::single(
        "generation-checked identities",
        "logical identities are pointer-free and generation checked",
    ),
    RequiredGroup::single("bounded generics", "uncontrolled specialization"),
    RequiredGroup::single("native call convention", "terlan-native-v2"),
    RequiredGroup::single("no native unwind", "no native unwinding crosses"),
    RequiredGroup::single(
        "inline scheduler poll",
        "common scheduler poll should be an inline",
    ),
    RequiredGroup::single(
        "local runtime fast path",
        "must not require canonical serialization",
    ),
    RequiredGroup::single("transport is not bytecode", "not bytecode"),
    RequiredGroup::single("atomic decoding", "decoding is atomic"),
    RequiredGroup::single(
        "external isolation",
        "outside the execution shard by default",
    ),
    RequiredGroup::single("hot-reload safety", "native memory is never reinterpreted"),
    RequiredGroup::single("rejection requirements", "rejection requirements"),
    RequiredGroup::single(
        "Erlang comparative gate",
        "supported erlang/otp build on identical hardware",
    ),
    RequiredGroup::single(
        "direct-object conformance",
        "secondary application compiler",
    ),
    RequiredGroup::single(
        "Cranelift-only code generation",
        "cranelift is the only conforming native code-generation backend",
    ),
    RequiredGroup::single("Terlan NativeIR", "terlan nativeir"),
    RequiredGroup::single(
        "Terlan owns its language ABI",
        "cranelift does not define the terlan language abi",
    ),
    RequiredGroup::single(
        "versioned Terlan stack maps",
        "compact, versioned terlan stack-map section",
    ),
];

/// Claims forbidden by the actor-local moving-heap architecture.
const NATIVE_ABI_FORBIDDEN_CLAIMS: &[&str] = &[
    "the abi 1 heap header is 32 bytes",
    "every heap object uses atomic reference counting",
    "niche optimizations are forbidden in abi 1",
    "every actor operation uses tvm transport encoding",
];

/// Forbidden claims that would make the artifact contract drift back to BEAM.
const FORBIDDEN_DEFAULT_CLAIMS: &[&str] = &[
    "tvm bytecode is the default executable image",
    "the vm interprets tvm instructions",
    "the final execution format is serialized vmir",
    "tvm executable images are json",
    "the tvm loader uses a jit",
];

/// Summary produced by the TVM executable-image contract check.
///
/// Inputs:
/// - Number of semantic groups enforced by the check.
///
/// Output:
/// - Stable success metric for CI output.
///
/// Transformation:
/// - Separates the validation count from failure diagnostics so the command
///   wrapper can print compact success text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmArtifactFormatSummary {
    pub required_group_count: usize,
}

/// Required phrase group for the artifact contract.
///
/// Inputs:
/// - `label`: human-readable requirement name.
/// - `phrases`: accepted lowercase text fragments.
///
/// Output:
/// - Immutable rule used by the contract checker.
///
/// Transformation:
/// - Stores one exact phrase for deterministic contract validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RequiredGroup {
    label: &'static str,
    phrases: RequiredPhrases,
}

/// Accepted phrase shape for one required group.
///
/// Inputs:
/// - Single phrase or multiple equivalent phrases.
///
/// Output:
/// - Static phrase holder for validation.
///
/// Transformation:
/// - Avoids allocating or borrowing temporary slices for one-phrase groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequiredPhrases {
    Single(&'static str),
}

impl RequiredGroup {
    /// Builds a required group with one accepted phrase.
    ///
    /// Inputs:
    /// - Requirement label.
    /// - Required lowercase phrase.
    ///
    /// Output:
    /// - Required group with one accepted phrase.
    ///
    /// Transformation:
    /// - Stores the phrase directly so validation can avoid temporary slices.
    const fn single(label: &'static str, phrase: &'static str) -> Self {
        Self {
            label,
            phrases: RequiredPhrases::Single(phrase),
        }
    }

    /// Returns whether the normalized document satisfies this group.
    ///
    /// Inputs:
    /// - Lowercase document text.
    ///
    /// Output:
    /// - `true` when any accepted phrase is present.
    ///
    /// Transformation:
    /// - Applies simple substring matching for stable documentation gates.
    fn matches(&self, normalized_text: &str) -> bool {
        match self.phrases {
            RequiredPhrases::Single(phrase) => normalized_text.contains(phrase),
        }
    }
}

/// Runs the TVM executable-image contract check.
///
/// Inputs:
/// - `root`: repository root containing `docs/runtime/`.
///
/// Output:
/// - Success summary when the artifact contract is present and explicit.
/// - Stable diagnostics when required contract language is missing or drifts
///   toward bytecode, serialized VMIR, JSON execution, or JIT loading.
///
/// Transformation:
/// - Reads the checked-in VM artifact contract and validates required semantic
///   groups plus forbidden default-runtime claims.
pub fn run_vm_artifact_format(root: &Path) -> QualityResult<VmArtifactFormatSummary> {
    let path = root.join(VM_ARTIFACT_FORMAT_DOC);
    let text = fs::read_to_string(&path).map_err(|err| {
        format!(
            "{}: failed to read TVM executable-image contract: {err}",
            path.display()
        )
    })?;
    let diagnostics = validate_vm_artifact_format_text(&text);
    if !diagnostics.is_empty() {
        return Err(render_vm_artifact_format_failure(&diagnostics));
    }
    let native_abi_path = root.join(VM_NATIVE_DATA_ABI_DOC);
    let native_abi_text = fs::read_to_string(&native_abi_path).map_err(|err| {
        format!(
            "{}: failed to read TVM native data ABI contract: {err}",
            native_abi_path.display()
        )
    })?;
    let native_abi_diagnostics = validate_native_data_abi_text(&native_abi_text);
    if !native_abi_diagnostics.is_empty() {
        return Err(render_vm_artifact_format_failure(&native_abi_diagnostics));
    }
    Ok(VmArtifactFormatSummary {
        required_group_count: REQUIRED_GROUPS.len() + NATIVE_ABI_REQUIRED_GROUPS.len(),
    })
}

/// Validates TVM executable-image contract text.
///
/// Inputs:
/// - `text`: documentation text.
///
/// Output:
/// - Diagnostics for missing required semantic groups or forbidden default
///   execution-format claims.
///
/// Transformation:
/// - Lowercases the text, checks required groups, then checks forbidden
///   phrases that would reintroduce BEAM/Erlang as the default runtime.
fn validate_vm_artifact_format_text(text: &str) -> Vec<String> {
    let normalized = normalize_contract_text(text);
    let mut diagnostics = Vec::new();
    for group in REQUIRED_GROUPS {
        if !group.matches(&normalized) {
            diagnostics.push(format!(
                "{}: missing VM artifact contract language",
                group.label
            ));
        }
    }
    for claim in FORBIDDEN_DEFAULT_CLAIMS {
        if normalized.contains(claim) {
            diagnostics.push(format!("forbidden default-runtime claim: `{claim}`"));
        }
    }
    diagnostics
}

/// Validates TVM native data ABI contract text.
///
/// Inputs:
/// - `text`: native value, call, ownership, and transition ABI specification.
///
/// Output:
/// - Diagnostics for missing actor-memory, native-call, isolation, or transport
///   requirements and for forbidden obsolete layout claims.
///
/// Transformation:
/// - Normalizes prose whitespace and enforces the independently versioned ABI
///   groups from the same release gate as the executable-image contract.
fn validate_native_data_abi_text(text: &str) -> Vec<String> {
    let normalized = normalize_contract_text(text);
    let mut diagnostics = Vec::new();
    for group in NATIVE_ABI_REQUIRED_GROUPS {
        if !group.matches(&normalized) {
            diagnostics.push(format!(
                "{}: missing TVM native data ABI contract language",
                group.label
            ));
        }
    }
    for claim in NATIVE_ABI_FORBIDDEN_CLAIMS {
        if normalized.contains(claim) {
            diagnostics.push(format!("forbidden native-ABI claim: `{claim}`"));
        }
    }
    diagnostics
}

/// Collapses case and whitespace for stable documentation-contract matching.
fn normalize_contract_text(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Renders VM artifact check diagnostics.
///
/// Inputs:
/// - `diagnostics`: individual validation failures.
///
/// Output:
/// - Stable multi-line failure block.
///
/// Transformation:
/// - Keeps Make and CI output readable without exposing internal data
///   structures.
fn render_vm_artifact_format_failure(diagnostics: &[String]) -> String {
    let mut message = String::from("[vm-artifact-format] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "vm_artifact_format_test.rs"]
mod vm_artifact_format_test;
