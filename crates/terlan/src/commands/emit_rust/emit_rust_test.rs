use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::formal_pipeline::compile_syntax_module_through_phases_with_profile;
use crate::terlan_hir::ModuleInterface;
use crate::terlan_typeck::{
    CoreExport, CoreExportKind, CoreExprSummary, CoreFunctionClause, CoreModuleMetadata, CoreParam,
    CoreProofCoverage, CoreProofReadiness, CoreSourceIdentity, CORE_IR_SCHEMA,
};
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::DiagnosticFormat;

/// Verifies Rust probe emission preserves CoreIR function visibility.
///
/// Inputs:
/// - A Terlan module with one public function and one private helper.
///
/// Output:
/// - Test passes when emitted Rust contains `pub fn` for the public
///   function and plain `fn` for the private helper.
///
/// Transformation:
/// - Compiles Terlan source through the formal pipeline, emits Rust from
///   CoreIR, and inspects the generated source surface.
#[test]
fn emit_core_module_to_rust_uses_core_function_visibility() {
    let source = "\
module rust_core_surface.

pub add(x: Int, y: Int): Int ->
    x + y.

hidden(x: Int): Int ->
    x.
";
    let rust = compile_source_to_rust_probe("rust_core_surface.terl", source);

    assert!(rust.contains("pub fn add(x: i64, y: i64) -> i64"), "{rust}");
    assert!(rust.contains("fn hidden(x: i64) -> i64"), "{rust}");
}

/// Verifies Rust probe emission handles direct pipe-forward CoreIR.
///
/// Inputs:
/// - A focused CoreIR module with a helper call and `|>` expression.
///
/// Output:
/// - Test passes when emitted Rust contains the expected local-call shape
///   and compiles as a Rust library.
///
/// Transformation:
/// - Bypasses parser syntax so the native probe directly exercises
///   `CoreExpr::BinaryOp` pipe lowering, then invokes `rustc --crate-type lib`
///   as a native neutrality check.
#[test]
fn emit_core_module_to_rust_compiles_pipe_forward_probe() {
    let module = core_module_with_functions(
        "rust_core_surface_pipe",
        vec![
            CoreFunction {
                name: "add".to_string(),
                arity: 2,
                public: true,
                params: vec![
                    CoreParam {
                        name: "x".to_string(),
                        ty: "Int".to_string(),
                        core_ty: Some(CoreType::Int),
                    },
                    CoreParam {
                        name: "y".to_string(),
                        ty: "Int".to_string(),
                        core_ty: Some(CoreType::Int),
                    },
                ],
                return_type: "Int".to_string(),
                core_return_type: Some(CoreType::Int),
                clauses: vec![CoreFunctionClause {
                    patterns: vec!["x".to_string(), "y".to_string()],
                    core_patterns: vec![
                        Some(CorePattern::Var("x".to_string())),
                        Some(CorePattern::Var("y".to_string())),
                    ],
                    pattern_proof_coverage: Vec::new(),
                    pattern_checked_preservation_evidence: Vec::new(),
                    guard: None,
                    body: direct_expr_summary(CoreExpr::BinaryOp {
                        operator: "+".to_string(),
                        left: Box::new(CoreExpr::Var("x".to_string())),
                        right: Box::new(CoreExpr::Var("y".to_string())),
                    }),
                }],
            },
            CoreFunction {
                name: "piped".to_string(),
                arity: 0,
                public: true,
                params: Vec::new(),
                return_type: "Int".to_string(),
                core_return_type: Some(CoreType::Int),
                clauses: vec![CoreFunctionClause {
                    patterns: Vec::new(),
                    core_patterns: Vec::new(),
                    pattern_proof_coverage: Vec::new(),
                    pattern_checked_preservation_evidence: Vec::new(),
                    guard: None,
                    body: direct_expr_summary(CoreExpr::BinaryOp {
                        operator: "|>".to_string(),
                        left: Box::new(CoreExpr::Int(1)),
                        right: Box::new(CoreExpr::Call {
                            function: "add".to_string(),
                            args: vec![CoreExpr::Int(2)],
                        }),
                    }),
                }],
            },
        ],
    );

    let rust = emit_core_module_to_rust(&module);

    assert!(rust.contains("pub fn piped() -> i64"), "{rust}");
    assert!(rust.contains("add(1, 2)"), "{rust}");
    assert_rust_probe_compiles(&rust);
}

/// Verifies Rust probe emission handles function-value invocation CoreIR.
///
/// Inputs:
/// - None; constructs a focused CoreIR module with a callable parameter.
///
/// Output:
/// - Test passes when emitted Rust contains `(f)(10)` and compiles.
///
/// Transformation:
/// - Bypasses parser syntax so the native probe directly exercises the
///   backend-neutral `CoreExpr::FunctionCall` payload.
#[test]
fn emit_core_module_to_rust_handles_function_value_call() {
    let module = core_module_with_functions(
        "rust_callable_probe",
        vec![CoreFunction {
            name: "apply".to_string(),
            arity: 1,
            public: true,
            params: vec![CoreParam {
                name: "f".to_string(),
                ty: "Term".to_string(),
                core_ty: Some(CoreType::Arrow {
                    params: vec![CoreType::Int],
                    return_type: Box::new(CoreType::Int),
                }),
            }],
            return_type: "Int".to_string(),
            core_return_type: Some(CoreType::Int),
            clauses: vec![CoreFunctionClause {
                patterns: vec!["f".to_string()],
                core_patterns: vec![Some(CorePattern::Var("f".to_string()))],
                pattern_proof_coverage: Vec::new(),
                pattern_checked_preservation_evidence: Vec::new(),
                guard: None,
                body: direct_expr_summary(CoreExpr::FunctionCall {
                    callee: Box::new(CoreExpr::Var("f".to_string())),
                    args: vec![CoreExpr::Int(10)],
                }),
            }],
        }],
    );

    let rust = emit_core_module_to_rust(&module);

    assert!(rust.contains("(f)(10)"), "{rust}");
    assert_rust_probe_compiles(&rust);
}

/// Verifies Rust probe emission escapes binary string literals portably.
///
/// Inputs:
/// - A focused CoreIR module returning a binary literal with quote, backslash,
///   newline, carriage return, and tab characters.
///
/// Output:
/// - Rust source containing a valid escaped string literal.
///
/// Transformation:
/// - Emits Rust from direct CoreIR and invokes `rustc --crate-type lib`, proving
///   the shared literal escaping helper produces compiler-accepted Rust source.
#[test]
fn emit_core_module_to_rust_escapes_binary_literals_portably() {
    let module = core_module_with_functions(
        "rust_core_surface_string_escape",
        vec![CoreFunction {
            name: "escaped".to_string(),
            arity: 0,
            public: true,
            params: Vec::new(),
            return_type: "Binary".to_string(),
            core_return_type: Some(CoreType::Binary),
            clauses: vec![CoreFunctionClause {
                patterns: Vec::new(),
                core_patterns: Vec::new(),
                pattern_proof_coverage: Vec::new(),
                pattern_checked_preservation_evidence: Vec::new(),
                guard: None,
                body: direct_expr_summary(CoreExpr::Binary(
                    "quote \" slash \\ newline \n carriage \r tab \t".to_string(),
                )),
            }],
        }],
    );

    let rust = emit_core_module_to_rust(&module);

    assert!(
        rust.contains(r#"String::from("quote \" slash \\ newline \n carriage \r tab \t")"#),
        "{rust}"
    );
    assert_rust_probe_compiles(&rust);
}

/// Verifies Rust probe emission handles selected primitive CoreIR intrinsics.
///
/// Inputs:
/// - A Terlan module that calls the receiver-style string method
///   `"hello".contains("ell")`.
///
/// Output:
/// - Test passes when emitted Rust contains the Rust string containment
///   operation and compiles as a library.
///
/// Transformation:
/// - Compiles source through parser, typechecking, and CoreIR lowering so
///   the receiver method becomes `core.string.contains`, then verifies the
///   Rust/native probe lowers that backend-neutral intrinsic.
#[test]
fn emit_core_module_to_rust_compiles_string_contains_intrinsic_probe() {
    let source = "\
module rust_core_surface_string_intrinsic.

pub has_needle(): Bool ->
    \"hello\".contains(\"ell\").
";
    let rust = compile_source_to_rust_probe("rust_core_surface_string_intrinsic.terl", source);

    assert!(rust.contains(".contains("), "{rust}");
    assert!(rust.contains(".as_str()"), "{rust}");
    assert_rust_probe_compiles(&rust);
}

/// Verifies Rust probe emission handles string-prefix primitive intrinsics.
///
/// Inputs:
/// - A Terlan module that calls the receiver-style string method
///   `"hello".starts_with("he")`.
///
/// Output:
/// - Test passes when emitted Rust uses the Rust string-prefix operation
///   and compiles as a library.
///
/// Transformation:
/// - Compiles source through parser, typechecking, and CoreIR lowering so
///   the receiver method becomes `core.string.starts_with`, then verifies
///   the Rust/native probe lowers that backend-neutral intrinsic.
#[test]
fn emit_core_module_to_rust_compiles_string_starts_with_intrinsic_probe() {
    let source = "\
module rust_core_surface_string_starts_with_intrinsic.

pub has_prefix(): Bool ->
    \"hello\".starts_with(\"he\").
";
    let rust = compile_source_to_rust_probe(
        "rust_core_surface_string_starts_with_intrinsic.terl",
        source,
    );

    assert!(rust.contains(".starts_with("), "{rust}");
    assert!(rust.contains(".as_str()"), "{rust}");
    assert_rust_probe_compiles(&rust);
}

/// Verifies Rust probe emission handles text-length primitive intrinsics.
///
/// Inputs:
/// - A Terlan module that calls the receiver-style string method
///   `"hello".length()`.
///
/// Output:
/// - Test passes when emitted Rust counts Unicode scalar values and
///   compiles as a library.
///
/// Transformation:
/// - Compiles source through parser, typechecking, and CoreIR lowering so
///   the receiver method becomes `core.string.length`, then verifies the
///   Rust/native probe lowers that backend-neutral intrinsic to a text
///   length expression rather than a byte-size expression.
#[test]
fn emit_core_module_to_rust_compiles_string_length_intrinsic_probe() {
    let source = "\
module rust_core_surface_string_length_intrinsic.

pub len(): Int ->
    \"hello\".length().
";
    let rust =
        compile_source_to_rust_probe("rust_core_surface_string_length_intrinsic.terl", source);

    assert!(rust.contains(".chars().count() as i64"), "{rust}");
    assert_rust_probe_compiles(&rust);
}

/// Compiles Terlan source into Rust probe source.
///
/// Inputs:
/// - `path`: diagnostic source path.
/// - `source`: Terlan module text.
///
/// Output:
/// - Rust source emitted from the checked CoreIR module.
///
/// Transformation:
/// - Runs the formal compiler pipeline with default profile settings and
///   passes the resulting CoreIR to the Rust/native probe emitter.
fn compile_source_to_rust_probe(path: &str, source: &str) -> String {
    let artifacts = compile_syntax_module_through_phases_with_profile(
        path,
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::default(),
    )
    .expect("compile source to CoreIR");

    emit_core_module_to_rust(&artifacts.core)
}

/// Builds a focused CoreModule for direct Rust probe tests.
///
/// Inputs:
/// - `module`: module identity to place in CoreIR metadata and interface.
/// - `functions`: already-constructed CoreIR function declarations.
///
/// Output:
/// - `CoreModule` with empty imports/types/constructors and zeroed metadata.
///
/// Transformation:
/// - Wraps direct test functions in the current CoreIR module shape without
///   depending on parser fixtures or backend-specific resolver data.
fn core_module_with_functions(module: &str, functions: Vec<CoreFunction>) -> CoreModule {
    CoreModule {
        schema: CORE_IR_SCHEMA.to_string(),
        module: module.to_string(),
        source: CoreSourceIdentity {
            source_kind: "direct-test".to_string(),
            syntax_contract_fingerprint: None,
        },
        imports: Vec::new(),
        exports: functions
            .iter()
            .filter(|function| function.public)
            .map(|function| CoreExport {
                name: function.name.clone(),
                kind: CoreExportKind::Function {
                    arity: function.arity,
                },
            })
            .collect(),
        types: Vec::new(),
        functions,
        constructors: Vec::new(),
        trait_conformances: Vec::new(),
        metadata: empty_core_metadata(),
        interface: empty_interface(module),
    }
}

/// Builds a minimal Core expression summary around a direct test expression.
///
/// Inputs:
/// - `expr`: typed CoreIR expression to attach to the summary.
///
/// Output:
/// - `CoreExprSummary` using Lean-covered test metadata.
///
/// Transformation:
/// - Packages a typed Core expression into the summary shape expected by
///   Core function clauses while leaving source-only metadata empty.
fn direct_expr_summary(expr: CoreExpr) -> CoreExprSummary {
    CoreExprSummary {
        kind: "direct-test".to_string(),
        core_expr: Some(expr),
        checked_preservation_evidence: None,
        proof_coverage: CoreProofCoverage::LeanCovered,
        text: None,
        remote: None,
        operator: None,
        arity: 0,
        children: Vec::new(),
    }
}

/// Builds zeroed Core module metadata for direct probe fixtures.
///
/// Inputs:
/// - None.
///
/// Output:
/// - `CoreModuleMetadata` with no counts and `LeanCovered` readiness.
///
/// Transformation:
/// - Provides deterministic metadata for tests that inspect emission rather
///   than proof accounting.
fn empty_core_metadata() -> CoreModuleMetadata {
    CoreModuleMetadata {
        interface_function_count: 0,
        interface_type_count: 0,
        constructor_count: 0,
        proof_readiness: CoreProofReadiness::LeanCovered,
        lean_covered_expr_count: 0,
        partial_expr_count: 0,
        proof_model_required_expr_count: 0,
        runtime_boundary_expr_count: 0,
        artifact_only_expr_count: 0,
        lean_covered_pattern_count: 0,
        partial_pattern_count: 0,
        proof_model_required_pattern_count: 0,
        runtime_boundary_pattern_count: 0,
        artifact_only_pattern_count: 0,
        typed_core_expr_count: 0,
        summary_only_expr_count: 0,
        typed_core_pattern_count: 0,
        summary_only_pattern_count: 0,
        typed_core_type_count: 0,
        summary_only_type_count: 0,
        checked_preservation_expr_count: 0,
        checked_preservation_pattern_count: 0,
        checked_preservation_expr_structural_count: 0,
        checked_preservation_pattern_structural_count: 0,
        checked_preservation_expr_no_runtime_bindings_count: 0,
        checked_preservation_pattern_no_runtime_bindings_count: 0,
        checked_preservation_expr_runtime_bindings_required_count: 0,
        checked_preservation_pattern_runtime_bindings_required_count: 0,
        resolved_constructor_call_identity_count: 0,
        resolved_constructor_chain_identity_count: 0,
        resolved_constructor_pattern_identity_count: 0,
        unresolved_constructor_call_candidate_count: 0,
        unresolved_constructor_chain_candidate_count: 0,
        unresolved_constructor_pattern_candidate_count: 0,
    }
}

/// Builds an empty module interface for direct probe fixtures.
///
/// Inputs:
/// - `module`: module identity for the interface.
///
/// Output:
/// - `ModuleInterface` with no public declarations.
///
/// Transformation:
/// - Uses empty maps and sets because Rust probe tests read CoreIR
///   functions directly and do not inspect interface rendering.
fn empty_interface(module: &str) -> ModuleInterface {
    ModuleInterface {
        module: module.to_string(),
        docs: Vec::new(),
        public_types: HashSet::new(),
        private_types: HashSet::new(),
        opaque_types: HashSet::new(),
        type_params: HashMap::new(),
        type_bodies: HashMap::new(),
        struct_fields: HashMap::new(),
        type_docs: HashMap::new(),
        traits: HashMap::new(),
        trait_conformances: Vec::new(),
        constructors: HashMap::new(),
        functions: HashMap::new(),
        function_overloads: HashMap::new(),
    }
}

/// Asserts that generated Rust probe source compiles as a library.
///
/// Inputs:
/// - `source`: generated Rust source text.
///
/// Output:
/// - Test assertion success when `rustc` exits successfully.
///
/// Transformation:
/// - Writes the source to a temporary file, invokes `rustc --crate-type lib`,
///   and removes the temporary directory after the check.
fn assert_rust_probe_compiles(source: &str) {
    let dir = unique_temp_dir("terlan_rust_probe");
    fs::create_dir_all(&dir).expect("create rust probe temp dir");
    let source_path = dir.join("probe.rs");
    let output_path = dir.join("libprobe.rlib");
    fs::write(&source_path, source).expect("write rust probe source");

    let output = Command::new("rustc")
        .arg("--edition=2021")
        .arg("--crate-type")
        .arg("lib")
        .arg(&source_path)
        .arg("-o")
        .arg(&output_path)
        .output()
        .expect("run rustc");
    let _ = fs::remove_dir_all(&dir);

    assert!(
        output.status.success(),
        "rustc failed\nsource:\n{}\nstdout:\n{}\nstderr:\n{}",
        source,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Builds a unique temporary directory path for test artifacts.
///
/// Inputs:
/// - `prefix`: human-readable temp directory prefix.
///
/// Output:
/// - Path in the process temp directory that is very unlikely to exist.
///
/// Transformation:
/// - Combines temp directory, process id, current nanosecond timestamp, and
///   caller prefix into a deterministic path string for this test run.
fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after unix epoch")
        .as_nanos();
    Path::new(&std::env::temp_dir()).join(format!("{}_{}_{}", prefix, std::process::id(), nanos))
}
