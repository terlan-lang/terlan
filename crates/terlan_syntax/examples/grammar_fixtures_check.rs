use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use serde::Deserialize;
use terlan_syntax::ast::Decl;
use terlan_syntax::parse_module;

#[derive(Deserialize)]
struct Manifest {
    version: Option<String>,
    fixtures: Vec<FixtureEntry>,
}

#[derive(Debug, Deserialize)]
struct AntlrExpectation {
    status: String,
    nodes: Vec<String>,
    message: Option<String>,
}

#[derive(Deserialize)]
struct FixtureEntry {
    id: String,
    class: String,
    expected: Option<Vec<String>>,
}

#[derive(Debug)]
struct Cli {
    manifest_path: PathBuf,
    fixture_root: PathBuf,
    antlr_frontend_path: Option<PathBuf>,
}

fn parse_args() -> Cli {
    let mut args = env::args().skip(1);
    let mut manifest = PathBuf::from("docs/grammar/fixtures/index.json");
    let mut fixture_root = PathBuf::from("docs/grammar/fixtures");
    let mut antlr_frontend_path = None;

    while let Some(flag) = args.next() {
        if flag == "--fixtures-root" {
            fixture_root = args
                .next()
                .unwrap_or_else(|| panic!("--fixtures-root requires path"))
                .into();
            continue;
        }
        if flag == "--antlr-json" {
            antlr_frontend_path = Some(
                args.next()
                    .unwrap_or_else(|| panic!("--antlr-json requires path"))
                    .into(),
            );
            continue;
        }

        if flag == "--help" {
            println!(
                "Usage: grammar_fixtures_check [manifest] [--fixtures-root <dir>] [--antlr-json <path>]"
            );
            println!(
                "  manifest: fixture manifest JSON (default docs/grammar/fixtures/index.json)"
            );
            println!("  --fixtures-root: fixture source directory (default docs/grammar/fixtures)");
            println!("  --antlr-json: optional ANTLR frontend expected-result JSON");
            std::process::exit(0);
        }

        manifest = flag.into();
    }

    Cli {
        manifest_path: manifest,
        fixture_root,
        antlr_frontend_path,
    }
}

#[derive(serde::Serialize)]
struct FixtureResult {
    id: String,
    class: String,
    status: String,
    nodes: Vec<String>,
    expected: Vec<String>,
    frontends: HashMap<String, FrontendResult>,
    message: Option<String>,
}

#[derive(Clone, serde::Serialize)]
struct FrontendResult {
    status: String,
    nodes: Vec<String>,
    message: Option<String>,
}

/// Maps one parsed declaration into the canonical fixture node class.
///
/// Inputs:
/// - `decl`: a Rust-parser declaration produced by `parse_module`.
///
/// Outputs:
/// - Canonical node class name used by grammar parity fixtures.
///
/// Transformation:
/// - Collapses parser-specific declaration variants into the cross-parser class
///   names expected by `docs/grammar/fixtures/index.json`.
fn decl_class(decl: &Decl) -> &'static str {
    match decl {
        Decl::Import(_) => "import_decl",
        Decl::Export(_) => panic!(
            "normal source grammar fixtures must not produce export_decl; use interface fixtures for export summaries"
        ),
        Decl::Type(t) if t.is_opaque => "opaque_type_decl",
        Decl::Type(_) => "type_decl",
        Decl::Struct(_) => "struct_decl",
        Decl::Constructor(_) => "constructor_decl",
        Decl::Function(_) => "function_decl",
        Decl::Trait(_) => "trait_decl",
        Decl::TraitImpl(_) => "trait_impl_decl",
        Decl::Method(_) => "method_decl",
        Decl::Template(_) => "template_decl",
        Decl::Raw(_) => "raw_decl",
    }
}

fn check_fixture(fixture: &FixtureEntry, source_root: &Path) -> FixtureResult {
    let fixture_path = source_root.join(format!("{}.ter", fixture.id));
    let source = match fs::read_to_string(&fixture_path) {
        Ok(source) => source,
        Err(error) => {
            return FixtureResult {
                id: fixture.id.clone(),
                class: fixture.class.clone(),
                status: "fail".into(),
                nodes: Vec::new(),
                frontends: HashMap::new(),
                expected: fixture.expected.clone().unwrap_or_default(),
                message: Some(format!("io error reading fixture: {error}")),
            };
        }
    };

    let nodes = match parse_module(&source) {
        Ok(module) => {
            let mut classes = Vec::new();
            for declaration in module.declarations.iter() {
                classes.push(decl_class(declaration).to_string());
            }
            Ok(classes)
        }
        Err(error) => Err(error),
    };

    let expected = fixture.expected.clone().unwrap_or_default();
    let mut result = FixtureResult {
        id: fixture.id.clone(),
        class: fixture.class.clone(),
        status: "ok".into(),
        nodes: Vec::new(),
        expected: expected.clone(),
        frontends: HashMap::new(),
        message: None,
    };

    let rust_result = match fixture.class.as_str() {
        "malformed" => match &nodes {
            Ok(_) => FrontendResult {
                status: "unexpected_success".into(),
                nodes: Vec::new(),
                message: Some("expected malformed fixture to fail, but parse succeeded".into()),
            },
            Err(_) => FrontendResult {
                status: "ok".into(),
                nodes: vec!["malformed".to_string()],
                message: None,
            },
        },
        "supported" | "raw" => match &nodes {
            Ok(nodes) => FrontendResult {
                status: "ok".into(),
                nodes: nodes.clone(),
                message: None,
            },
            Err(_) => FrontendResult {
                status: "parse_error".into(),
                nodes: Vec::new(),
                message: Some("parse error in rust frontend".into()),
            },
        },
        _ => FrontendResult {
            status: "unknown_fixture_class".into(),
            nodes: Vec::new(),
            message: Some(format!("unknown fixture class '{}'", fixture.class)),
        },
    };
    result.frontends.insert("rust".into(), rust_result);

    match fixture.class.as_str() {
        "malformed" => match nodes {
            Ok(nodes) => {
                result.status = "fail".into();
                result.nodes = nodes;
                result.message =
                    Some("expected malformed fixture to fail, but parse succeeded".into());
            }
            Err(_error) => {
                result.status = "ok".into();
                result.nodes = vec!["malformed".to_string()];
            }
        },
        "supported" | "raw" => match nodes {
            Ok(nodes) => {
                result.nodes = nodes;
                if !expected.is_empty() && result.nodes != expected {
                    result.status = "fail".into();
                    result.message = Some("node class sequence does not match expected".into());
                    if let Some(rust) = result.frontends.get_mut("rust") {
                        rust.status = "class_mismatch".into();
                    }
                }
            }
            Err(error) => {
                result.status = "fail".into();
                result.message = Some(format!(
                    "parser failure: {} @ {:?}",
                    error.message, error.span
                ));
            }
        },
        _ => {
            result.status = "fail".into();
            result.message = Some(format!("unknown fixture class '{}'", fixture.class));
        }
    }
    result
}

fn main() {
    let cli = parse_args();
    let antlr_frontend: HashMap<String, AntlrExpectation> = match cli.antlr_frontend_path.as_ref() {
        Some(path) => match fs::read_to_string(path) {
            Ok(data) => match serde_json::from_str::<HashMap<String, AntlrExpectation>>(&data) {
                Ok(parsed) => parsed,
                Err(err) => {
                    eprintln!("cannot parse ANTLR fixture JSON {}: {err}", path.display());
                    process::exit(1);
                }
            },
            Err(err) => {
                eprintln!("cannot read ANTLR fixture JSON {}: {err}", path.display());
                process::exit(1);
            }
        },
        None => HashMap::new(),
    };

    let manifest_data = match fs::read_to_string(&cli.manifest_path) {
        Ok(data) => data,
        Err(err) => {
            eprintln!(
                "cannot read manifest {}: {err}",
                cli.manifest_path.display()
            );
            process::exit(1);
        }
    };

    let manifest: Manifest = match serde_json::from_str(&manifest_data) {
        Ok(manifest) => manifest,
        Err(err) => {
            eprintln!("invalid manifest {}: {err}", cli.manifest_path.display());
            process::exit(1);
        }
    };

    let _ = &manifest.version;

    let mut failures = Vec::new();
    let mut totals = HashMap::new();
    for fixture in manifest.fixtures {
        let mut verdict = check_fixture(&fixture, &cli.fixture_root);

        if let Some(antlr_expected) = antlr_frontend.get(&fixture.id) {
            let _ = &antlr_expected.message;
            let (antlr_status, antlr_nodes, antlr_message) = match fixture.class.as_str() {
                "malformed" => {
                    if antlr_expected.status == "error" {
                        ("ok".to_string(), Vec::new(), None)
                    } else {
                        (
                            "mismatch".to_string(),
                            antlr_expected.nodes.clone(),
                            Some(format!(
                                "expected ANTLR status error but got status '{}'",
                                antlr_expected.status
                            )),
                        )
                    }
                }
                "supported" | "raw" => {
                    if antlr_expected.status == "ok" && verdict.nodes == antlr_expected.nodes {
                        ("ok".to_string(), antlr_expected.nodes.clone(), None)
                    } else {
                        (
                            "mismatch".to_string(),
                            antlr_expected.nodes.clone(),
                            Some(format!(
                                "ANTLR nodes {:?} do not match Rust parser {:?}",
                                antlr_expected.nodes, verdict.nodes
                            )),
                        )
                    }
                }
                _ => (
                    "skip".to_string(),
                    Vec::new(),
                    Some(format!("unknown fixture class '{}'", fixture.class)),
                ),
            };
            if antlr_status != "ok"
                && !antlr_message
                    .as_ref()
                    .is_some_and(|message| message.is_empty())
            {
                verdict.status = "fail".to_string();
            }
            verdict.frontends.insert(
                "antlr".into(),
                FrontendResult {
                    status: antlr_status.clone(),
                    nodes: antlr_nodes,
                    message: antlr_message,
                },
            );
            if verdict.status == "ok" && fixture.class.as_str() != "malformed" {
                if let Some(rust) = verdict.frontends.get("rust") {
                    if rust.status == "ok"
                        && antlr_status == "ok"
                        && verdict.nodes != antlr_expected.nodes
                    {
                        verdict.status = "fail".into();
                    }
                }
            }
        } else {
            verdict.frontends.insert(
                "antlr".into(),
                FrontendResult {
                    status: "not_configured".into(),
                    nodes: Vec::new(),
                    message: None,
                },
            );
        }

        totals
            .entry(verdict.status.clone())
            .and_modify(|count| *count += 1)
            .or_insert(1usize);
        if verdict.status == "fail" {
            failures.push(verdict.id.clone());
        }
        match serde_json::to_string(&verdict) {
            Ok(line) => println!("{line}"),
            Err(err) => {
                eprintln!("failed to serialize result for {}: {err}", verdict.id);
                process::exit(1);
            }
        }
    }

    if !failures.is_empty() {
        eprintln!(
            "fixture check failed for {} fixture(s): {:?}",
            failures.len(),
            failures
        );
        process::exit(1);
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&totals).expect("totals should serialize")
    );
    process::exit(0);
}
