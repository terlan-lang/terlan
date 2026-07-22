use super::*;
use crate::commands::static_site::*;
use crate::support::test_fs;
use crate::terlan_hir::resolve_syntax_module_output_with_interfaces;
use crate::terlan_syntax::parse_module_as_syntax_output;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::validation::template_contract::type_check_syntax_module_output_with_templates;

/// Creates a command-test temporary directory.
fn make_temp_dir(name: &str) -> PathBuf {
    test_fs::temp_dir("tests", name)
}

/// Writes one Terlan fixture file and returns its path string.
fn fixture(path: &Path, contents: &str) -> String {
    let file = path.join("fixture.terl");
    test_fs::write_file(&file, contents);
    file.to_string_lossy().to_string()
}

mod check_constructor_error_manifest_test;
mod check_constructor_identity_manifest_test;
mod check_incremental_test;
mod check_language_feature_rejection_test;
mod check_phase_manifest_smoke_test;
mod check_phase_test;
mod check_std_vm_test;
mod check_target_profile_gate_test;
mod check_target_profile_progression_test;
mod command_transition_test;
mod debug_cli_test;
mod doc_test;
mod emit_js_test;
mod help_test;
mod interface_test;
mod static_site_test;
mod target_profile_test;

/// Phase-contract fixture descriptor used by compiler pipeline tests.
struct PhaseContractFixture {
    module_name: &'static str,
    source_path: &'static str,
}

/// Returns all phase-contract fixtures in stable execution order.
fn phase_contract_fixtures() -> Vec<PhaseContractFixture> {
    vec![
        PhaseContractFixture {
            module_name: "phase_basic",
            source_path: "phase_basic.terl",
        },
        PhaseContractFixture {
            module_name: "phase_binary_eq",
            source_path: "phase_binary_eq.terl",
        },
        PhaseContractFixture {
            module_name: "phase_binary_lt",
            source_path: "phase_binary_lt.terl",
        },
        PhaseContractFixture {
            module_name: "phase_binary_lte",
            source_path: "phase_binary_lte.terl",
        },
        PhaseContractFixture {
            module_name: "phase_binary_gt",
            source_path: "phase_binary_gt.terl",
        },
        PhaseContractFixture {
            module_name: "phase_binary_gte",
            source_path: "phase_binary_gte.terl",
        },
        PhaseContractFixture {
            module_name: "phase_binary_mul",
            source_path: "phase_binary_mul.terl",
        },
        PhaseContractFixture {
            module_name: "phase_binary_sub",
            source_path: "phase_binary_sub.terl",
        },
        PhaseContractFixture {
            module_name: "phase_core_lean",
            source_path: "phase_core_lean.terl",
        },
        PhaseContractFixture {
            module_name: "phase_int_literal",
            source_path: "phase_int_literal.terl",
        },
        PhaseContractFixture {
            module_name: "phase_atom_literal",
            source_path: "phase_atom_literal.terl",
        },
        PhaseContractFixture {
            module_name: "phase_binary_literal",
            source_path: "phase_binary_literal.terl",
        },
        PhaseContractFixture {
            module_name: "phase_tuple_literal",
            source_path: "phase_tuple_literal.terl",
        },
        PhaseContractFixture {
            module_name: "phase_list_literal",
            source_path: "phase_list_literal.terl",
        },
        PhaseContractFixture {
            module_name: "phase_named_call",
            source_path: "phase_named_call.terl",
        },
        PhaseContractFixture {
            module_name: "phase_core_lambda",
            source_path: "phase_core_lambda.terl",
        },
        PhaseContractFixture {
            module_name: "phase_unary_operator",
            source_path: "phase_unary_operator.terl",
        },
        PhaseContractFixture {
            module_name: "phase_list_cons",
            source_path: "phase_list_cons.terl",
        },
        PhaseContractFixture {
            module_name: "phase_if_expr",
            source_path: "phase_if_expr.terl",
        },
        PhaseContractFixture {
            module_name: "phase_field_access",
            source_path: "phase_field_access.terl",
        },
        PhaseContractFixture {
            module_name: "phase_literal_pattern_case",
            source_path: "phase_literal_pattern_case.terl",
        },
        PhaseContractFixture {
            module_name: "phase_no_expressions",
            source_path: "phase_no_expressions.terl",
        },
        PhaseContractFixture {
            module_name: "phase_summary_type_debt",
            source_path: "phase_summary_type_debt.terl",
        },
        PhaseContractFixture {
            module_name: "phase_template",
            source_path: "phase_template.terl",
        },
        PhaseContractFixture {
            module_name: "phase_constructor_resolution",
            source_path: "phase_constructor_resolution.terl",
        },
        PhaseContractFixture {
            module_name: "phase_constructor_pattern_resolution",
            source_path: "phase_constructor_pattern_resolution.terl",
        },
        PhaseContractFixture {
            module_name: "phase_constructor_chain_resolution",
            source_path: "phase_constructor_chain_resolution.terl",
        },
        PhaseContractFixture {
            module_name: "phase_trait",
            source_path: "phase_trait.terl",
        },
    ]
}

/// Returns the repository fixture root for phase-contract tests.
fn phase_contract_fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/phase_contract")
}

/// Reads one expected phase-contract golden snapshot.
fn read_phase_contract_golden(name: &str, stage: &str) -> String {
    let path = phase_contract_fixture_root().join(format!("{name}.{stage}.golden"));
    fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!("failed to read phase contract golden {path:?}: {err}");
    })
}

/// Lowers a phase-contract fixture into deterministic CoreIR contract text.
///
/// Inputs:
/// - `fixture`: phase-contract fixture descriptor with module name and
///   source path relative to the phase-contract fixture root.
///
/// Output:
/// - Deterministic `CoreModule::contract_text()` for the parsed, resolved,
///   and CoreIR-lowered fixture.
///
/// Transformation:
/// - Reads the fixture source, parses it into syntax output, resolves it
///   with local interfaces, lowers the resolved typed module into CoreIR,
///   and returns the CoreIR contract snapshot used by formal proof gates.
fn phase_contract_core_contract_text(fixture: &PhaseContractFixture) -> String {
    let root = phase_contract_fixture_root();
    let source_path = root.join(fixture.source_path);
    let source = fs::read_to_string(&source_path)
        .unwrap_or_else(|err| panic!("failed to read phase fixture {source_path:?}: {err}"));
    let syntax_output =
        formal_pipeline::parse_source_as_syntax_output(&source_path.to_string_lossy(), &source)
            .unwrap_or_else(|err| {
                panic!(
                    "failed to parse syntax output fixture {}: {err:?}",
                    fixture.source_path
                )
            });
    let interfaces =
        formal_pipeline::load_external_interfaces(&source_path.to_string_lossy(), None);
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax_output, &interfaces).module;
    crate::terlan_typeck::lower_syntax_module_output_to_core(&syntax_output, &resolved)
        .contract_text()
}

/// Runs `check --emit-phase-manifest` for a phase-contract fixture.
///
/// Inputs:
/// - `fixture`: phase-contract fixture descriptor with module name and
///   source path relative to the phase-contract fixture root.
///
/// Output:
/// - Parsed JSON phase manifest emitted by the CLI check command.
///
/// Transformation:
/// - Executes the same command-level check path used by external tooling,
///   writes the manifest to a temporary path, reads it back, and parses it
///   into JSON so tests can assert command-artifact proof coverage.
fn phase_contract_check_manifest_json(fixture: &PhaseContractFixture) -> serde_json::Value {
    let root = phase_contract_fixture_root();
    let source_path = root.join(fixture.source_path);
    let dir = make_temp_dir(&format!("{}_phase_manifest", fixture.module_name));
    let manifest = dir.join(format!("{}.phase-manifest.json", fixture.module_name));
    let cache = dir.join("cache");

    let exit = commands::check::run(
        CliCommand {
            verb: Some("check".into()),
            args: vec![
                source_path.to_string_lossy().into(),
                "--emit-phase-manifest".into(),
                manifest.to_string_lossy().into(),
            ],
        },
        CliState {
            cache_dir: Some(cache),
            ..Default::default()
        },
    );
    assert_eq!(exit, ExitCode::SUCCESS);

    let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
    serde_json::from_str(&manifest_text).expect("parse phase manifest")
}

/// Normalizes golden snapshot text for stable comparisons.
fn normalize_golden_text(text: &str) -> String {
    text.lines()
        .map(|line| line.trim_end())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// Renders a deterministic resolver-stage snapshot.
fn resolve_stage_snapshot(resolved: &crate::terlan_hir::ResolvedModule) -> String {
    let mut out = Vec::new();
    out.push(format!("module={}", resolved.name));
    out.push(format!("diagnostics={}", resolved.diagnostics.len()));
    let mut function_keys = resolved
        .function_symbols
        .iter()
        .map(|(key, symbol)| {
            (
                key.0.clone(),
                key.1,
                symbol.public,
                symbol.exported,
                symbol.return_type.clone(),
                symbol
                    .params
                    .iter()
                    .map(|param| format!("{}:{}", param.name, param.annotation))
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    function_keys.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    out.push(format!("function_symbols={}", function_keys.len()));
    for (name, arity, public, exported, return_type, params) in function_keys {
        out.push(format!(
            "fn={}/{} public={} exported={} return={}",
            name, arity, public, exported, return_type
        ));
        for param in params {
            out.push(format!("  param={}", param));
        }
    }

    let mut local_types = resolved
        .local_type_names
        .iter()
        .map(|(name, vis)| format!("{name}:{vis:?}"))
        .collect::<Vec<_>>();
    local_types.sort();
    out.push(format!("local_types={}", local_types.join(",")));

    let mut imported_types = resolved
        .imported_types
        .iter()
        .map(|(name, imported)| {
            format!(
                "{}:{}:{}",
                name, imported.source_module, imported.visibility as i32
            )
        })
        .collect::<Vec<_>>();
    imported_types.sort();
    out.push(format!("imported_types={}", imported_types.join(",")));

    let mut imported_traits = resolved
        .imported_traits
        .iter()
        .map(|(name, imported)| {
            format!(
                "{}:{}:{}",
                name, imported.source_module, imported.visibility as i32
            )
        })
        .collect::<Vec<_>>();
    imported_traits.sort();
    out.push(format!("imported_traits={}", imported_traits.join(",")));

    let mut interface_map = resolved.interface_map.keys().cloned().collect::<Vec<_>>();
    interface_map.sort();
    out.push(format!("interface_map={}", interface_map.join(",")));
    out.push(format!(
        "interface_functions={}",
        resolved.interface.functions.len()
    ));
    normalize_golden_text(&out.join("\n"))
}

/// Renders a deterministic typecheck diagnostic snapshot.
fn typed_stage_snapshot(diagnostics: &[crate::terlan_typeck::Diagnostic]) -> String {
    if diagnostics.is_empty() {
        return "diagnostics=ok\n".to_string();
    }
    let mut entries = diagnostics
        .iter()
        .map(|diagnostic| {
            let severity = match diagnostic.severity {
                crate::terlan_typeck::DiagSeverity::Error => "error",
                crate::terlan_typeck::DiagSeverity::Warning => "warning",
            };
            format!(
                "{}:{}-{}:{}",
                severity, diagnostic.span.start, diagnostic.span.end, diagnostic.message
            )
        })
        .collect::<Vec<_>>();
    entries.sort();
    normalize_golden_text(&entries.join("\n"))
}

/// Renders a deterministic CoreIR stage snapshot.
fn core_stage_snapshot(core: &crate::terlan_typeck::CoreModule) -> String {
    normalize_golden_text(&core.contract_text())
}

/// Asserts one fixture matches every phase-contract golden snapshot.
fn assert_phase_contract_golden(fixture: PhaseContractFixture) {
    let root = phase_contract_fixture_root();
    let update_goldens = std::env::var_os("TERLAN_UPDATE_PHASE_GOLDEN").is_some();
    let source_path = root.join(fixture.source_path);
    let source = fs::read_to_string(&source_path).unwrap_or_else(|err| {
        panic!("failed to read phase fixture source {source_path:?}: {err}");
    });
    let syntax_output =
        formal_pipeline::parse_source_as_syntax_output(&source_path.to_string_lossy(), &source)
            .unwrap_or_else(|err| {
                panic!(
                    "failed to parse syntax output fixture {}: {err:?}",
                    fixture.source_path
                )
            });

    let interfaces =
        formal_pipeline::load_external_interfaces(&source_path.to_string_lossy(), None);
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax_output, &interfaces).module;
    let resolved_snapshot = resolve_stage_snapshot(&resolved);
    let expected_resolve = read_phase_contract_golden(fixture.module_name, "resolve");
    if update_goldens {
        let golden_path = root.join(format!("{}.resolve.golden", fixture.module_name));
        fs::write(&golden_path, &resolved_snapshot).expect("write resolve phase golden");
    } else {
        assert_eq!(resolved_snapshot, normalize_golden_text(&expected_resolve));
    }

    let diagnostics =
        type_check_syntax_module_output_with_templates(&syntax_output, &resolved, &source_path);
    let typed_snapshot = typed_stage_snapshot(&diagnostics);
    let expected_typed = read_phase_contract_golden(fixture.module_name, "typed");
    if update_goldens {
        let golden_path = root.join(format!("{}.typed.golden", fixture.module_name));
        fs::write(&golden_path, &typed_snapshot).expect("write typed phase golden");
    } else {
        assert_eq!(typed_snapshot, normalize_golden_text(&expected_typed));
    }

    let core = crate::terlan_typeck::lower_syntax_module_output_to_core(&syntax_output, &resolved);
    let core_snapshot = core_stage_snapshot(&core);
    let expected_core = read_phase_contract_golden(fixture.module_name, "core");
    if update_goldens {
        let golden_path = root.join(format!("{}.core.golden", fixture.module_name));
        fs::write(&golden_path, &core_snapshot).expect("write core phase golden");
    } else {
        assert_eq!(core_snapshot, normalize_golden_text(&expected_core));
    }
}

/// Verifies all phase-contract fixtures match checked-in golden snapshots.
#[test]
fn run_phase_contract_fixtures_match_golden() {
    for fixture in phase_contract_fixtures() {
        assert_phase_contract_golden(fixture);
    }
}

/// Verifies LP8 CoreIR-to-Lean conformance baselines stay Lean-covered.
///
/// Inputs:
/// - `phase_core_lean`: simple function fixture that exercises direct
///   Lean-covered variable CoreIR.
/// - `phase_core_lambda`: anonymous-function fixture that exercises
///   runtime-binding freshness evidence for lambda lowering.
/// - `phase_constructor_resolution`: resolved constructor-call fixture
///   that exercises Lean-covered constructor values.
/// - `phase_constructor_pattern_resolution`: resolved constructor-pattern
///   fixture that exercises case-pattern runtime-binding freshness.
///
/// Output:
/// - Test assertion only; no source or golden files are modified.
///
/// Transformation:
/// - Lowers each fixture through the formal parse/resolve/typecheck/CoreIR
///   path and checks the resulting CoreIR contract text for the proof
///   readiness and freshness snippets required by the Lean handoff.
#[test]
fn run_phase_contract_lean_conformance_baselines_are_lean_covered() {
    for baseline in validation::proof_baseline::contract_baselines() {
        let fixture = phase_contract_fixtures()
            .into_iter()
            .find(|fixture| fixture.module_name == baseline.module_name)
            .unwrap_or_else(|| panic!("missing Lean conformance fixture {}", baseline.module_name));
        let core_contract = phase_contract_core_contract_text(&fixture);

        validation::proof_baseline::validate_contract_baseline(baseline, &core_contract)
            .unwrap_or_else(|err| panic!("{err}:\n{core_contract}"));
    }
}

/// Verifies the next LP8 Lean-model candidate has stable typed CoreIR.
///
/// Inputs:
/// - `phase_basic`: arithmetic fixture that currently lowers to typed
///   `BinaryOp` CoreIR with Lean-covered variable children.
///
/// Output:
/// - Test assertion only; no source or golden files are modified.
///
/// Transformation:
/// - Lowers each candidate fixture through the formal
///   parse/resolve/typecheck/CoreIR path and checks that the resulting
///   contract remains typed, preservation-backed, and
///   `proof-model-required` until Lean models that CoreIR form.
#[test]
fn run_phase_contract_next_lean_model_candidates_are_pinned() {
    for baseline in validation::proof_baseline::next_lean_model_candidate_baselines() {
        let fixture = phase_contract_fixtures()
            .into_iter()
            .find(|fixture| fixture.module_name == baseline.module_name)
            .unwrap_or_else(|| panic!("missing Lean model candidate {}", baseline.module_name));
        let core_contract = phase_contract_core_contract_text(&fixture);

        validation::proof_baseline::validate_contract_baseline(baseline, &core_contract)
            .unwrap_or_else(|err| panic!("{err}:\n{core_contract}"));
    }
}

/// Verifies LP8 Lean conformance baselines are visible in phase manifests.
///
/// Inputs:
/// - `phase_core_lean`: simple function fixture that should emit one
///   Lean-covered expression and one Lean-covered pattern.
/// - `phase_core_lambda`: anonymous-function fixture that should emit two
///   Lean-covered expressions with one runtime-binding freshness
///   obligation.
/// - `phase_constructor_resolution`: resolved constructor-call fixture
///   that should emit one resolved constructor-call identity.
/// - `phase_constructor_pattern_resolution`: resolved constructor-pattern
///   fixture that should emit one resolved constructor-pattern identity
///   and case runtime-binding freshness evidence.
///
/// Output:
/// - Test assertion only; no source or golden files are modified.
///
/// Transformation:
/// - Runs each fixture through command-level `check --emit-phase-manifest`
///   and verifies the manifest `core_proof_coverage` counters match the
///   CoreIR Lean-conformance baseline expected by external proof tooling.
#[test]
fn run_check_phase_contract_lean_conformance_baselines_emit_manifest_evidence() {
    for baseline in validation::proof_baseline::manifest_baselines() {
        let fixture = phase_contract_fixtures()
            .into_iter()
            .find(|fixture| fixture.module_name == baseline.module_name)
            .unwrap_or_else(|| panic!("missing Lean conformance fixture {}", baseline.module_name));
        let manifest_json = phase_contract_check_manifest_json(&fixture);

        validation::proof_baseline::validate_manifest_baseline_artifact(
            baseline,
            manifest_json["core_ir_hash"].as_u64(),
            manifest_json["core_proof_coverage"]["readiness"].as_str(),
            |field| manifest_json["core_proof_coverage"][field].as_u64(),
        )
        .unwrap_or_else(|err| panic!("{err}"));
    }
}

/// Verifies next LP8 Lean-model candidates are visible in phase manifests.
///
/// Inputs:
/// - `phase_trait`: trait fixture that should emit one
///   proof-model-required remote/scoped-call expression and Lean-covered
///   variable argument children.
///
/// Output:
/// - Test assertion only; no source or golden files are modified.
///
/// Transformation:
/// - Runs each candidate fixture through command-level
///   `check --emit-phase-manifest` and verifies the manifest
///   `core_proof_coverage` counters match the candidate baseline while the
///   readiness remains `proof-model-required`.
#[test]
fn run_check_phase_contract_next_lean_model_candidates_emit_manifest_evidence() {
    for baseline in validation::proof_baseline::next_lean_model_candidate_manifest_baselines() {
        let fixture = phase_contract_fixtures()
            .into_iter()
            .find(|fixture| fixture.module_name == baseline.module_name)
            .unwrap_or_else(|| panic!("missing Lean model candidate {}", baseline.module_name));
        let manifest_json = phase_contract_check_manifest_json(&fixture);

        validation::proof_baseline::validate_manifest_baseline_artifact_with_readiness(
            baseline,
            "proof-model-required",
            manifest_json["core_ir_hash"].as_u64(),
            manifest_json["core_proof_coverage"]["readiness"].as_str(),
            |field| manifest_json["core_proof_coverage"][field].as_u64(),
        )
        .unwrap_or_else(|err| panic!("{err}"));
    }
}
