use super::*;

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use crate::commands::static_site::*;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};
    use terlan_hir::resolve_syntax_module_output_with_interfaces;
    use terlan_syntax::{
        parse_module_as_syntax_output, SyntaxDeclarationPayload, SyntaxModuleOutput,
    };

    use crate::validation::template_contract::type_check_syntax_module_output_with_templates;

    fn make_temp_dir(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        path.push(format!(
            "terlan_cli_tests_{}_{}_{}",
            name,
            std::process::id(),
            now
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    fn fixture(path: &Path, contents: &str) -> String {
        let file = path.join("fixture.terl");
        fs::write(&file, contents).expect("write fixture");
        file.to_string_lossy().to_string()
    }

    mod check_constructor_error_manifest_test;
    mod check_constructor_identity_manifest_test;
    mod check_incremental_test;
    mod check_language_feature_rejection_test;
    mod check_phase_test;
    mod check_target_profile_gate_test;
    mod check_target_profile_progression_test;
    mod command_transition_test;
    mod doc_test;
    mod emit_js_test;
    mod help_test;
    mod interface_test;
    mod static_site_test;
    mod target_profile_test;

    struct PhaseContractFixture {
        module_name: &'static str,
        source_path: &'static str,
    }

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

    fn phase_contract_fixture_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/phase_contract")
    }

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
        let resolved =
            resolve_syntax_module_output_with_interfaces(&syntax_output, &interfaces).module;
        terlan_typeck::lower_syntax_module_output_to_core(&syntax_output, &resolved).contract_text()
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

    fn normalize_golden_text(text: &str) -> String {
        text.lines()
            .map(|line| line.trim_end())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    }

    fn syntax_public_function_surface_snapshot(module: &SyntaxModuleOutput) -> Vec<String> {
        let mut entries = module
            .declarations
            .iter()
            .filter_map(|decl| match &decl.payload {
                SyntaxDeclarationPayload::Function {
                    name,
                    params,
                    is_public,
                    ..
                } if *is_public => Some(format!("{}/{}", name, params.len())),
                _ => None,
            })
            .collect::<Vec<_>>();
        entries.sort();
        entries
    }

    /// Builds the expected exported Erlang function surface for one syntax
    /// fixture.
    ///
    /// Inputs:
    /// - `module`: syntax-output module fixture.
    ///
    /// Output:
    /// - Sorted Erlang export names including public source functions with
    ///   hidden trait-evidence arguments and constructor helper exports.
    ///
    /// Transformation:
    /// - Derives public function arity from source parameters plus runtime
    ///   trait-evidence parameters, then appends deterministic constructor
    ///   helper names for public constructors.
    fn syntax_public_erlang_surface_snapshot(module: &SyntaxModuleOutput) -> Vec<String> {
        let mut entries = module
            .declarations
            .iter()
            .filter_map(|decl| match &decl.payload {
                SyntaxDeclarationPayload::Function {
                    name,
                    params,
                    generic_bounds,
                    is_public,
                    ..
                } if *is_public => {
                    Some(format!("{}/{}", name, params.len() + generic_bounds.len()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        for decl in &module.declarations {
            match &decl.payload {
                SyntaxDeclarationPayload::Constructor {
                    name,
                    is_public,
                    clauses,
                    ..
                } if *is_public => {
                    for clause in clauses {
                        let fixed_arity = clause
                            .params
                            .iter()
                            .filter(|param| !param.is_varargs)
                            .count();
                        let varargs = clause.params.iter().any(|param| param.is_varargs);
                        let emitted_arity = if varargs {
                            fixed_arity + 1
                        } else {
                            fixed_arity
                        };
                        entries.push(format!(
                            "{}/{}",
                            phase_contract_constructor_function_name(name, fixed_arity, varargs),
                            emitted_arity
                        ));
                    }
                }
                _ => {}
            }
        }
        entries.sort();
        entries
    }

    /// Maps a public constructor declaration to the emitted helper name used by
    /// phase-contract backend surface checks.
    ///
    /// Inputs:
    /// - `name`: source constructor name.
    /// - `fixed_arity`: number of non-vararg constructor parameters.
    /// - `varargs`: whether the constructor accepts a vararg parameter.
    ///
    /// Output:
    /// - Erlang/JavaScript helper function name expected in backend exports.
    ///
    /// Transformation:
    /// - Mirrors the backend's deterministic constructor helper naming scheme
    ///   for phase-contract tests without depending on backend-private helpers.
    fn phase_contract_constructor_function_name(
        name: &str,
        fixed_arity: usize,
        varargs: bool,
    ) -> String {
        if varargs {
            format!(
                "typer_ctor_{}_varargs_{}",
                phase_contract_erlang_type_name(name),
                fixed_arity
            )
        } else {
            format!(
                "typer_ctor_{}_{}",
                phase_contract_erlang_type_name(name),
                fixed_arity
            )
        }
    }

    /// Converts a source constructor name into the backend helper stem used by
    /// phase-contract tests.
    ///
    /// Inputs:
    /// - `name`: source constructor name.
    ///
    /// Output:
    /// - Lowercase snake-style backend type-name stem.
    ///
    /// Transformation:
    /// - Inserts underscores before non-leading uppercase ASCII letters and
    ///   lowercases uppercase ASCII letters, matching backend helper naming.
    fn phase_contract_erlang_type_name(name: &str) -> String {
        let mut out = String::new();
        for (idx, ch) in name.chars().enumerate() {
            if ch.is_ascii_uppercase() {
                if idx > 0 {
                    out.push('_');
                }
                out.push(ch.to_ascii_lowercase());
            } else {
                out.push(ch);
            }
        }
        out
    }

    fn resolve_stage_snapshot(resolved: &terlan_hir::ResolvedModule) -> String {
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

    fn typed_stage_snapshot(diagnostics: &[terlan_typeck::Diagnostic]) -> String {
        if diagnostics.is_empty() {
            return "diagnostics=ok\n".to_string();
        }
        let mut entries = diagnostics
            .iter()
            .map(|diagnostic| {
                let severity = match diagnostic.severity {
                    terlan_typeck::DiagSeverity::Error => "error",
                    terlan_typeck::DiagSeverity::Warning => "warning",
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

    fn core_stage_snapshot(core: &terlan_typeck::CoreModule) -> String {
        normalize_golden_text(&core.contract_text())
    }

    fn emit_stage_snapshot(path: &Path) -> String {
        let source = fs::read_to_string(path).unwrap_or_else(|err| {
            panic!("failed to read emitted file {path:?}: {err}");
        });
        let mut out = Vec::new();
        for line in source.lines() {
            let trimmed = line.trim_end();
            if trimmed.starts_with("-module(")
                || trimmed.starts_with("-export(")
                || (trimmed.ends_with(" ->") && !trimmed.starts_with(" "))
            {
                out.push(trimmed.to_string());
            }
        }
        if out.is_empty() {
            panic!("no emit snapshot lines found in {path:?}");
        }
        normalize_golden_text(&out.join("\n"))
    }

    fn parse_erlang_exported_function_surface(path: &Path) -> Vec<String> {
        let source = fs::read_to_string(path).unwrap_or_else(|err| {
            panic!("failed to read emitted erlang file {path:?}: {err}");
        });
        let mut exports = Vec::new();
        for line in source.lines() {
            let trimmed = line.trim();
            let Some(body) = trimmed.strip_prefix("-export([") else {
                continue;
            };
            let Some(body) = body.strip_suffix("]).") else {
                continue;
            };
            if body.trim().is_empty() {
                continue;
            }
            for entry in body.split(',') {
                let entry = entry.trim();
                if entry.is_empty() {
                    continue;
                }
                if let Some((name, arity)) = entry.rsplit_once('/') {
                    if !name.is_empty() && !arity.is_empty() {
                        exports.push(entry.to_string());
                    }
                }
            }
        }
        exports.sort();
        exports
    }

    fn parse_js_exported_function_surface(path: &Path) -> Vec<String> {
        let source = fs::read_to_string(path).unwrap_or_else(|err| {
            panic!("failed to read emitted js file {path:?}: {err}");
        });
        let mut exports = Vec::new();
        for line in source.lines() {
            let trimmed = line.trim();
            let Some(rest) = trimmed.strip_prefix("export function ") else {
                continue;
            };
            let Some(paren_start) = rest.find('(') else {
                continue;
            };
            let function_name = rest[..paren_start].trim();
            if function_name.is_empty() {
                continue;
            }
            let rest = &rest[paren_start + 1..];
            let Some(paren_end) = rest.find(')') else {
                continue;
            };
            let params = rest[..paren_end].trim();
            let arity = if params.is_empty() {
                0
            } else {
                params.split(',').count()
            };
            exports.push(format!("{function_name}/{arity}"));
        }
        exports.sort();
        exports
    }

    /// Extracts public function names from backend surface entries.
    ///
    /// Inputs:
    /// - `surface`: sorted backend export entries formatted as `name/arity`.
    ///
    /// Output:
    /// - Sorted function names with backend arity removed.
    ///
    /// Transformation:
    /// - Splits each surface entry at the final `/`, keeps the function-name
    ///   prefix, sorts the names, and removes duplicates so cross-backend
    ///   checks compare source-visible names rather than backend ABI arity.
    fn public_function_names_from_surface(surface: &[String]) -> Vec<String> {
        let mut names = surface
            .iter()
            .filter_map(|entry| {
                entry
                    .rsplit_once('/')
                    .map(|(name, _arity)| name.to_string())
            })
            .collect::<Vec<_>>();
        names.sort();
        names.dedup();
        names
    }

    fn assert_phase_contract_golden(fixture: PhaseContractFixture) {
        let root = phase_contract_fixture_root();
        let update_goldens = std::env::var_os("TERRLANG_UPDATE_PHASE_GOLDEN").is_some();
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
        let resolved =
            resolve_syntax_module_output_with_interfaces(&syntax_output, &interfaces).module;
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

        let core = terlan_typeck::lower_syntax_module_output_to_core(&syntax_output, &resolved);
        let core_snapshot = core_stage_snapshot(&core);
        let expected_core = read_phase_contract_golden(fixture.module_name, "core");
        if update_goldens {
            let golden_path = root.join(format!("{}.core.golden", fixture.module_name));
            fs::write(&golden_path, &core_snapshot).expect("write core phase golden");
        } else {
            assert_eq!(core_snapshot, normalize_golden_text(&expected_core));
        }

        let out_dir = make_temp_dir("phase_contract_emit");
        let exit = commands::emit::run(
            CliCommand {
                verb: Some("emit".into()),
                args: vec![source_path.to_string_lossy().to_string()],
            },
            CliState {
                out_dir: out_dir.clone(),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::SUCCESS);
        let emitted_path = out_dir.join(format!(
            "{}.erl",
            support::erlang_output_stem(&syntax_output.module_name)
        ));
        let emit_snapshot = emit_stage_snapshot(&emitted_path);
        let expected_emit = read_phase_contract_golden(fixture.module_name, "emit");
        if update_goldens {
            let golden_path = root.join(format!("{}.emit.golden", fixture.module_name));
            fs::write(&golden_path, &emit_snapshot).expect("write emit phase golden");
        } else {
            assert_eq!(emit_snapshot, normalize_golden_text(&expected_emit));
        }
    }

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
                .unwrap_or_else(|| {
                    panic!("missing Lean conformance fixture {}", baseline.module_name)
                });
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
                .unwrap_or_else(|| {
                    panic!("missing Lean conformance fixture {}", baseline.module_name)
                });
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

    #[test]
    fn run_phase_contract_fixtures_backend_parity() {
        for fixture in phase_contract_fixtures() {
            let root = phase_contract_fixture_root();
            let source_path = root.join(fixture.source_path);
            let source = fs::read_to_string(&source_path).unwrap_or_else(|err| {
                panic!("failed to read phase fixture {source_path:?}: {err}")
            });
            let syntax_output = formal_pipeline::parse_source_as_syntax_output(
                &source_path.to_string_lossy(),
                &source,
            )
            .unwrap_or_else(|err| {
                panic!(
                    "failed to parse syntax output fixture {}: {err:?}",
                    fixture.source_path
                )
            });
            let expected_js_surface = syntax_public_function_surface_snapshot(&syntax_output);
            let expected_erlang_surface = syntax_public_erlang_surface_snapshot(&syntax_output);
            let interfaces =
                formal_pipeline::load_external_interfaces(&source_path.to_string_lossy(), None);
            let resolved =
                resolve_syntax_module_output_with_interfaces(&syntax_output, &interfaces).module;
            let core = terlan_typeck::lower_syntax_module_output_to_core(&syntax_output, &resolved);
            let erlang_interfaces = interfaces.into_iter().collect::<BTreeMap<_, _>>();
            let direct_erlang =
                terlan_erlang::try_emit_syntax_module_output_to_erlang_with_interfaces_file_imports_templates_and_markdown(
                    &syntax_output,
                    &erlang_interfaces,
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                )
                .unwrap_or_else(|err| {
                    panic!("failed direct Erlang lowering for {source_path:?}: {err}")
                });
            let core_gated_erlang =
                terlan_erlang::try_emit_core_module_to_erlang_with_syntax_bridge(
                    &core,
                    &syntax_output,
                    &erlang_interfaces,
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                )
                .unwrap_or_else(|err| {
                    panic!("failed CoreIR-gated Erlang lowering for {source_path:?}: {err}")
                });
            assert_eq!(
                core_gated_erlang, direct_erlang,
                "CoreIR-gated Erlang output drift for {:?}",
                source_path
            );

            let erlang_dir = make_temp_dir("backend_parity_erlang");
            assert_eq!(
                commands::emit::run(
                    CliCommand {
                        verb: Some("emit".into()),
                        args: vec![source_path.to_string_lossy().to_string()],
                    },
                    CliState {
                        out_dir: erlang_dir.clone(),
                        ..Default::default()
                    },
                ),
                ExitCode::SUCCESS
            );
            let erlang_path = erlang_dir.join(format!(
                "{}.erl",
                support::erlang_output_stem(&syntax_output.module_name)
            ));
            let erlang_surface = parse_erlang_exported_function_surface(&erlang_path);
            assert_eq!(
                erlang_surface, expected_erlang_surface,
                "erlang surface mismatch for {:?}",
                source_path
            );

            let js_dir = make_temp_dir("backend_parity_js");
            assert_eq!(
                commands::emit_js::run(
                    &[
                        source_path.to_string_lossy().to_string(),
                        "--declarations".into(),
                    ],
                    &CliState {
                        out_dir: js_dir.clone(),
                        ..Default::default()
                    },
                ),
                ExitCode::SUCCESS
            );
            let js_path = js_dir.join(format!("{}.js", syntax_output.module_name));
            let js_source = fs::read_to_string(&js_path)
                .unwrap_or_else(|err| panic!("failed to read emitted js file {js_path:?}: {err}"));
            commands::emit_js::assert_oxc_accepts_js_artifact(&js_path, &js_source);
            let js_surface = parse_js_exported_function_surface(&js_path);
            assert_eq!(
                js_surface, expected_js_surface,
                "js surface mismatch for {:?}",
                source_path
            );
            let erlang_public_names = public_function_names_from_surface(&erlang_surface);
            for public_function in public_function_names_from_surface(&js_surface) {
                assert!(
                    erlang_public_names.contains(&public_function),
                    "Erlang surface missing public JS function name {public_function} for {:?}",
                    source_path
                );
            }

            let declarations_path = js_dir.join(format!("{}.d.ts", syntax_output.module_name));
            let declarations = fs::read_to_string(&declarations_path).unwrap_or_else(|err| {
                panic!("failed to read ts declarations {declarations_path:?}: {err}")
            });
            let expected_declarations_empty =
                core.types.iter().all(|type_decl| {
                    !matches!(type_decl.visibility, terlan_typeck::CoreVisibility::Public)
                }) && core.functions.iter().all(|function| !function.public);
            if expected_declarations_empty {
                assert!(
                    declarations.is_empty(),
                    "expected empty declarations for fixture with no public CoreIR declaration surface {:?}",
                    source_path
                );
            } else {
                assert!(
                    !declarations.is_empty(),
                    "expected declarations for fixture with public CoreIR declaration surface {:?}",
                    source_path
                );
            }
        }
    }

    /// Verifies executable Task operations fail in target-profile validation
    /// before backend emission.
    ///
    /// Inputs:
    /// - A temporary Terlan module importing `std.core.Task` and calling
    ///   `Task.spawn(() -> 1)` in a function body.
    ///
    /// Output:
    /// - Test assertion only; `terlc check --emit-phase-manifest` must fail
    ///   with parse, resolve, and typecheck phases complete and the CoreIR
    ///   target-profile phase marked as an error.
    ///
    /// Transformation:
    /// - Runs the public command path and confirms the formal std Task
    ///   contract remains importable/typecheckable while runtime Task
    ///   execution for unsupported Task operations is rejected until backend
    ///   support exists.
    #[test]
    fn run_check_single_file_rejects_task_operation_before_backend_emission() {
        let dir = make_temp_dir("check_single_file_task_operation_rejected");
        let source = dir.join("task_operation.terl");
        fs::write(
            &source,
            "\
module task_operation.\n\
\n\
import std.core.Task.\n\
\n\
pub complete(): Task[Int] ->\n\
    Task.spawn(() -> 1).\n",
        )
        .expect("write task operation source");
        let manifest = dir.join("task_operation.phase-manifest.json");

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState::default(),
        );

        assert_ne!(exit, ExitCode::SUCCESS);
        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"resolve","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"error""#));
        assert!(manifest_text.contains("task operation std.core.Task.spawn"));
    }

    /// Verifies BEAM Agent paired state/value operations pass target-profile
    /// validation once backend lowering exists.
    ///
    /// Inputs:
    /// - A temporary Terlan module importing `std.beam.Agent` and calling
    ///   `Agent.get_and_update(...)` in a function body.
    ///
    /// Output:
    /// - Test assertion only; `terlc check --emit-phase-manifest` must pass
    ///   with parse, resolve, typecheck, and CoreIR phases complete.
    ///
    /// Transformation:
    /// - Runs the public command path and confirms `get_and_update` is part of
    ///   the admitted Agent runtime surface instead of being rejected as a
    ///   deferred process-backed operation.
    #[test]
    fn run_check_single_file_accepts_beam_agent_get_and_update_before_backend_emission() {
        let dir = make_temp_dir("check_single_file_beam_agent_get_and_update_accepted");
        let source = dir.join("beam_agent_operation.terl");
        fs::write(
            &source,
            "\
module beam_agent_operation.\n\
\n\
import std.beam.Agent.\n\
\n\
pub queue_update(agent: Agent[Int]): Int ->\n\
    Agent.get_and_update(agent, (value: Int) -> {value, value}).\n",
        )
        .expect("write BEAM Agent operation source");
        let manifest = dir.join("beam_agent_operation.phase-manifest.json");

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState::default(),
        );

        assert_eq!(exit, ExitCode::SUCCESS);
        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"resolve","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        assert!(!manifest_text.contains("BEAM Agent operation std.beam.Agent.get_and_update"));
    }

    /// Verifies BEAM GenServer implementations can rely on default callbacks.
    ///
    /// Inputs:
    /// - A temporary Terlan module importing `std.beam.GenServer` and
    ///   implementing `GenServer[...]` without a `terminate` method.
    ///
    /// Output:
    /// - Test assertion only; `terlc check --emit-phase-manifest` must pass
    ///   with parse, resolve, typecheck, and CoreIR phases complete.
    ///
    /// Transformation:
    /// - Runs the public command path and confirms generated `.typi` default
    ///   method markers make optional BEAM callbacks usable outside the
    ///   typechecker unit-test path.
    #[test]
    fn run_check_single_file_accepts_beam_gen_server_default_terminate_before_backend_emission() {
        let dir = make_temp_dir("check_single_file_beam_gen_server_default_terminate_accepted");
        let source = dir.join("beam_gen_server_default_terminate.terl");
        fs::write(
            &source,
            "\
module beam_gen_server_default_terminate.\n\
\n\
import std.beam.GenServer.{GenServer, CallReply}.\n\
import std.core.Result.{Result, Ok}.\n\
import std.core.Error.{Error}.\n\
\n\
pub struct CounterServer implements GenServer[CounterServer, Int, Int, Int, Int] {\n\
    seed: Int\n\
}.\n\
\n\
pub (server: CounterServer) init(): Result[Int, Error] ->\n\
    Ok(server.seed).\n\
\n\
pub (server: CounterServer) handle_call(state: Int, request: Int): Result[CallReply[Int, Int], Error] ->\n\
    Ok({state, request}).\n\
\n\
pub (server: CounterServer) handle_cast(state: Int, event: Int): Result[Int, Error] ->\n\
    Ok(state + event).\n",
        )
        .expect("write BEAM GenServer source");
        let manifest = dir.join("beam_gen_server_default_terminate.phase-manifest.json");

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState::default(),
        );

        assert_eq!(exit, ExitCode::SUCCESS);
        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"resolve","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
    }

    /// Verifies executable GenServer operations pass after runtime lowering.
    ///
    /// Inputs:
    /// - A temporary Terlan module importing `std.beam.GenServer` and calling
    ///   `GenServer.start(server)`.
    ///
    /// Output:
    /// - Test assertion only; `terlc check --emit-phase-manifest` must pass
    ///   with parse, resolve, typecheck, and CoreIR phases complete.
    ///
    /// Transformation:
    /// - Runs the public command path and confirms GenServer process startup
    ///   is admitted after callback dispatch lowering is implemented by the
    ///   BEAM backend.
    #[test]
    fn run_check_single_file_accepts_beam_gen_server_operation_before_backend_emission() {
        let dir = make_temp_dir("check_single_file_beam_gen_server_operation_accepted");
        let source = dir.join("beam_gen_server_operation.terl");
        fs::write(
            &source,
            "\
module beam_gen_server_operation.\n\
\n\
import std.beam.GenServer.\n\
import type std.beam.GenServer.{CallReply, GenServer, ServerRef}.\n\
import std.core.Result.{Ok}.\n\
import type std.core.Result.Result.\n\
import type std.core.Error.Error.\n\
\n\
pub struct CounterServer implements GenServer[CounterServer, Int, Int, Int, Int] {\n\
    seed: Int\n\
}.\n\
\n\
pub (server: CounterServer) init(): Result[Int, Error] ->\n\
    Ok(server.seed).\n\
\n\
pub (server: CounterServer) handle_call(state: Int, request: Int): Result[CallReply[Int, Int], Error] ->\n\
    Ok({state, request}).\n\
\n\
pub (server: CounterServer) handle_cast(state: Int, event: Int): Result[Int, Error] ->\n\
    Ok(state + event).\n\
\n\
pub start_server(server: CounterServer): Dynamic ->\n\
    GenServer.start(server).\n",
        )
        .expect("write BEAM GenServer operation source");
        let manifest = dir.join("beam_gen_server_operation.phase-manifest.json");

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState::default(),
        );

        assert_eq!(exit, ExitCode::SUCCESS);
        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"resolve","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        assert!(!manifest_text.contains("BEAM GenServer operation std.beam.GenServer.start"));
    }

    /// Verifies executable BEAM Task operations pass target-profile validation
    /// once shared process lowering exists.
    ///
    /// Inputs:
    /// - A temporary Terlan module importing `std.beam.Task` and calling
    ///   `Task.start(() -> 1)` in a function body.
    ///
    /// Output:
    /// - Test assertion only; `terlc check --emit-phase-manifest` must pass
    ///   with parse, resolve, typecheck, and CoreIR phases complete.
    ///
    /// Transformation:
    /// - Runs the public command path and confirms BEAM Task process-backed
    ///   execution is admitted after the shared BEAM process intrinsic layer
    ///   owns lowering.
    #[test]
    fn run_check_single_file_accepts_beam_task_operation_before_backend_emission() {
        let dir = make_temp_dir("check_single_file_beam_task_operation_accepted");
        let source = dir.join("beam_task_operation.terl");
        fs::write(
            &source,
            "\
module beam_task_operation.\n\
\n\
import std.beam.Task.\n\
\n\
pub start_work(): Dynamic ->\n\
    Task.start(() -> 1).\n",
        )
        .expect("write BEAM Task operation source");
        let manifest = dir.join("beam_task_operation.phase-manifest.json");

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState::default(),
        );

        assert_eq!(exit, ExitCode::SUCCESS);
        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"resolve","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        assert!(!manifest_text.contains("BEAM Task operation std.beam.Task.start"));
    }

    /// Verifies NativeBridge runtime operations pass public check after local lowering exists.
    ///
    /// Inputs:
    /// - A temporary Terlan module importing `std.beam.NativeBridge` and
    ///   calling `NativeBridge.start(resource)`.
    ///
    /// Output:
    /// - Test assertion only; `terlc check --emit-phase-manifest` must pass
    ///   under the Erlang profile with parse, resolve, typecheck, and CoreIR
    ///   phases all marked ok.
    ///
    /// Transformation:
    /// - Runs the public command path and confirms the callable NativeBridge
    ///   contract is visible to source and admitted by the Erlang target
    ///   profile before backend emission.
    #[test]
    fn run_check_single_file_accepts_beam_native_bridge_operation_before_backend_emission() {
        let dir = make_temp_dir("check_single_file_beam_native_bridge_operation_accepted");
        let source = dir.join("beam_native_bridge_operation.terl");
        fs::write(
            &source,
            "\
module beam_native_bridge_operation.\n\
\n\
import std.beam.NativeBridge.\n\
import type std.beam.NativeBridge.NativeBridge.\n\
\n\
pub start_bridge(resource: String): Dynamic ->\n\
    NativeBridge.start(resource).\n",
        )
        .expect("write BEAM NativeBridge operation source");
        let manifest = dir.join("beam_native_bridge_operation.phase-manifest.json");

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState::default(),
        );

        assert_eq!(exit, ExitCode::SUCCESS);
        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"resolve","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        assert!(!manifest_text.contains("BEAM NativeBridge operation std.beam.NativeBridge.start"));
    }

    /// Verifies Supervisor runtime operations pass public check after local lowering exists.
    ///
    /// Inputs:
    /// - A temporary Terlan module importing `std.beam.Supervisor` and calling
    ///   `Supervisor.child_spec(value)`.
    ///
    /// Output:
    /// - Test assertion only; `terlc check --emit-phase-manifest` must pass
    ///   under the Erlang profile with parse, resolve, typecheck, and CoreIR
    ///   phases all marked ok.
    ///
    /// Transformation:
    /// - Runs the public command path and confirms the callable Supervisor
    ///   contract is visible to source and admitted by the Erlang target
    ///   profile before backend emission.
    #[test]
    fn run_check_single_file_accepts_beam_supervisor_operation_before_backend_emission() {
        let dir = make_temp_dir("check_single_file_beam_supervisor_operation_accepted");
        let source = dir.join("beam_supervisor_operation.terl");
        fs::write(
            &source,
            "\
module beam_supervisor_operation.\n\
\n\
import std.beam.Supervisor.\n\
\n\
pub make_spec(value: Int): Dynamic ->\n\
    Supervisor.child_spec(value).\n",
        )
        .expect("write BEAM Supervisor operation source");
        let manifest = dir.join("beam_supervisor_operation.phase-manifest.json");

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState::default(),
        );

        assert_eq!(exit, ExitCode::SUCCESS);
        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"resolve","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        assert!(!manifest_text.contains("BEAM Supervisor operation std.beam.Supervisor.child_spec"));
    }

    /// Verifies config metadata entries are visible but non-semantic in 0.0.1.
    ///
    /// Inputs:
    /// - A temporary single-file Terlan module containing a `target` config
    ///   declaration with structured metadata entries and one simple function.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds, records a
    ///   warning in the typecheck phase, and still lowers the function to CoreIR.
    ///
    /// Transformation:
    /// - Runs the generic formal compiler path and confirms config entries are
    ///   preserved as source metadata instead of being silently treated as backend
    ///   semantics.
    #[test]
    fn run_check_single_file_warns_for_unconsumed_config_entries_in_phase_manifest() {
        let dir = make_temp_dir("check_single_file_config_entries_warn");
        let source = dir.join("config_entries.terl");
        fs::write(
            &source,
            "module config_entries.\n\ntarget erlang {\n  otp_application: true;\n  features: [sockets]\n}.\n\npub value(): Int ->\n  1.\n",
        )
        .expect("write config entry source");
        let manifest = dir.join("config_entries.phase-manifest.json");

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState::default(),
        );

        assert_eq!(exit, ExitCode::SUCCESS);
        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""code":"type_warning""#));
        assert!(manifest_text.contains("config metadata entries for `target erlang`"));
        assert!(manifest_text.contains("preserved but not semantically consumed"));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
    }

    /// Verifies a declaration-only check emits a no-expressions Core manifest.
    ///
    /// Inputs:
    /// - A temporary single-file Terlan module containing only a public type
    ///   alias.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
    ///   manifest reports CoreIR readiness as `no-expressions` with one typed
    ///   CoreType payload and no expression or pattern payloads.
    ///
    /// Transformation:
    /// - Runs the CLI check command through the formal pipeline and validates
    ///   the emitted phase-manifest JSON.
    #[test]
    fn run_check_single_file_type_only_emits_no_expressions_manifest() {
        let dir = make_temp_dir("check_single_file_no_expressions_manifest");
        let source = dir.join("type_only.terl");
        fs::write(&source, "module type_only.\n\npub type UserId = Int.\n")
            .expect("write type-only source");
        let manifest = dir.join("type_only.phase-manifest.json");

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState::default(),
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        assert_ne!(
            manifest_json["core_ir_hash"]
                .as_u64()
                .expect("core ir hash"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["readiness"]
                .as_str()
                .expect("core proof readiness"),
            "no-expressions"
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["typed_core_type"]
                .as_u64()
                .expect("typed CoreType count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["summary_only_type"]
                .as_u64()
                .expect("summary-only CoreType count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["typed_core_expr"]
                .as_u64()
                .expect("typed CoreExpr count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["typed_core_pattern"]
                .as_u64()
                .expect("typed CorePattern count"),
            0
        );
    }

    /// Verifies declaration-only summary type debt reaches phase manifests.
    ///
    /// Inputs:
    /// - A temporary single-file Terlan module containing only a public struct
    ///   declaration whose body is not yet modeled as typed CoreType.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
    ///   manifest reports CoreIR readiness as `proof-model-required` with one
    ///   summary-only CoreType payload.
    ///
    /// Transformation:
    /// - Runs the CLI check command through the formal pipeline and validates
    ///   that type-model debt prevents declaration-only CoreIR from reporting
    ///   `no-expressions`.
    #[test]
    fn run_check_single_file_struct_only_emits_typed_struct_body_manifest() {
        let dir = make_temp_dir("check_single_file_summary_type_debt_manifest");
        let source = dir.join("struct_only.terl");
        fs::write(
            &source,
            "module struct_only.\n\npub struct Point {\n    x: Int,\n    y: Int\n}.\n",
        )
        .expect("write struct-only source");
        let manifest = dir.join("struct_only.phase-manifest.json");

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState::default(),
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        assert_ne!(
            manifest_json["core_ir_hash"]
                .as_u64()
                .expect("core ir hash"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["readiness"]
                .as_str()
                .expect("core proof readiness"),
            "no-expressions"
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["typed_core_type"]
                .as_u64()
                .expect("typed CoreType count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["summary_only_type"]
                .as_u64()
                .expect("summary-only CoreType count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["typed_core_expr"]
                .as_u64()
                .expect("typed CoreExpr count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["typed_core_pattern"]
                .as_u64()
                .expect("typed CorePattern count"),
            0
        );
    }

    /// Verifies lambda freshness obligations reach phase manifests.
    ///
    /// Inputs:
    /// - A temporary single-file Terlan module whose public function returns
    ///   an anonymous function expression with one runtime parameter binding.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
    ///   manifest partitions expression preservation evidence into one
    ///   no-runtime-binding child and one runtime-binding lambda root.
    ///
    /// Transformation:
    /// - Runs the CLI check command through the formal pipeline and validates
    ///   the freshness buckets future Lean proof export will need for lambda
    ///   substitution evidence.
    #[test]
    fn run_check_single_file_lambda_emits_runtime_binding_freshness_manifest() {
        let dir = make_temp_dir("check_single_file_lambda_freshness_manifest");
        let source = dir.join("lambda_freshness.terl");
        fs::write(
            &source,
            "module lambda_freshness.\n\npub id_fun(): Term ->\n    (x) -> x.\n",
        )
        .expect("write lambda freshness source");
        let manifest = dir.join("lambda_freshness.phase-manifest.json");

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState::default(),
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        assert_eq!(
            manifest_json["core_proof_coverage"]["readiness"]
                .as_str()
                .expect("core proof readiness"),
            "lean-covered"
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["typed_core_expr"]
                .as_u64()
                .expect("typed CoreExpr count"),
            2
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["checked_preservation_expr"]
                .as_u64()
                .expect("checked-preservation expression count"),
            2
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["checked_preservation_expr_no_runtime_bindings"]
                .as_u64()
                .expect("no-runtime-bindings expression count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]
                ["checked_preservation_expr_runtime_bindings_required"]
                .as_u64()
                .expect("runtime-bindings-required expression count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["checked_preservation_pattern"]
                .as_u64()
                .expect("checked-preservation pattern count"),
            0
        );
    }
}
