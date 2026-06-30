use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::terlan_syntax::SyntaxContractArtifactCheck;

use super::{
    parse_syntax_contract_command, run, syntax_contract_command_output, syntax_contract_file_check,
    SyntaxContractCommand, SyntaxContractCommandParseError, SyntaxContractOutputMode,
};

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

fn temp_file(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "terlan_syntax_contract_{name}_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ))
}

#[test]
fn parse_syntax_contract_command_defaults_to_artifact_emit() {
    assert_eq!(
        parse_syntax_contract_command(&[]),
        Ok(SyntaxContractCommand::Emit {
            mode: SyntaxContractOutputMode::ArtifactJson,
            out_path: None
        })
    );
}

#[test]
fn parse_syntax_contract_command_accepts_fingerprint_and_output_path() {
    assert_eq!(
        parse_syntax_contract_command(&args(&["--fingerprint", "--out", "contract.txt"])),
        Ok(SyntaxContractCommand::Emit {
            mode: SyntaxContractOutputMode::Fingerprint,
            out_path: Some(PathBuf::from("contract.txt"))
        })
    );
}

#[test]
fn parse_syntax_contract_command_accepts_check_path() {
    assert_eq!(
        parse_syntax_contract_command(&args(&["--check", "contract.json"])),
        Ok(SyntaxContractCommand::Check {
            path: PathBuf::from("contract.json")
        })
    );
}

#[test]
fn parse_syntax_contract_command_rejects_invalid_flag_combinations() {
    for invalid in [
        vec!["--unknown"],
        vec!["--fingerprint", "--fingerprint"],
        vec!["--out"],
        vec!["--out", "one.json", "--out", "two.json"],
        vec!["--check"],
        vec!["--fingerprint", "--check", "contract.json"],
        vec!["--check", "contract.json", "--out", "copy.json"],
    ] {
        assert_eq!(
            parse_syntax_contract_command(&args(&invalid)),
            Err(SyntaxContractCommandParseError),
            "invalid args should fail: {invalid:?}"
        );
    }
}

#[test]
fn syntax_contract_command_output_emits_json_artifact_and_fingerprint() {
    let json = syntax_contract_command_output(SyntaxContractOutputMode::ArtifactJson)
        .expect("artifact json");
    let fingerprint =
        syntax_contract_command_output(SyntaxContractOutputMode::Fingerprint).expect("fingerprint");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("artifact JSON");

    assert_eq!(parsed["schema"], "terlan-syntax-contract-v1");
    assert_eq!(parsed["fingerprint"], fingerprint);
    assert!(!fingerprint.is_empty());
    let Some(hex) = fingerprint.strip_prefix("fnv1a64:") else {
        panic!("unexpected fingerprint format: {fingerprint}");
    };
    assert!(hex.chars().all(|ch| ch.is_ascii_hexdigit()));
}

#[test]
fn syntax_contract_file_check_accepts_artifact_json_and_fingerprint() {
    let artifact_path = temp_file("artifact.json");
    let fingerprint_path = temp_file("fingerprint.txt");
    let artifact = syntax_contract_command_output(SyntaxContractOutputMode::ArtifactJson)
        .expect("artifact json");
    let fingerprint =
        syntax_contract_command_output(SyntaxContractOutputMode::Fingerprint).expect("fingerprint");
    fs::write(&artifact_path, &artifact).expect("write artifact");
    fs::write(&fingerprint_path, &fingerprint).expect("write fingerprint");

    assert_eq!(
        syntax_contract_file_check(&artifact_path),
        Ok(SyntaxContractArtifactCheck::Match {
            fingerprint: fingerprint.clone()
        })
    );
    assert_eq!(
        syntax_contract_file_check(&fingerprint_path),
        Ok(SyntaxContractArtifactCheck::Match { fingerprint })
    );

    let _ = fs::remove_file(&artifact_path);
    let _ = fs::remove_file(&fingerprint_path);
}

#[test]
fn syntax_contract_file_check_reports_invalid_and_mismatched_artifacts() {
    let invalid_path = temp_file("invalid.txt");
    let mismatch_path = temp_file("mismatch.txt");
    fs::write(&invalid_path, "not a contract").expect("write invalid");
    let mut artifact: serde_json::Value = serde_json::from_str(
        &syntax_contract_command_output(SyntaxContractOutputMode::ArtifactJson)
            .expect("artifact json"),
    )
    .expect("artifact value");
    artifact["fingerprint"] = serde_json::Value::String("fnv1a64:0000000000000000".to_string());
    fs::write(&mismatch_path, artifact.to_string()).expect("write mismatch");

    assert_eq!(
        syntax_contract_file_check(&invalid_path),
        Ok(SyntaxContractArtifactCheck::InvalidArtifact)
    );
    match syntax_contract_file_check(&mismatch_path).expect("mismatch check") {
        SyntaxContractArtifactCheck::Mismatch { expected, found } => {
            assert_ne!(expected, found);
            assert_eq!(found, "fnv1a64:0000000000000000");
        }
        other => panic!("expected mismatch, got {other:?}"),
    }

    let _ = fs::remove_file(&invalid_path);
    let _ = fs::remove_file(&mismatch_path);
}

#[test]
fn run_writes_artifact_to_output_file_and_rejects_invalid_args() {
    let output_path = temp_file("run-output.json");
    let exit = run(&args(&[
        "--out",
        output_path.to_str().expect("utf8 output path"),
    ]));

    assert_eq!(exit, ExitCode::SUCCESS);
    let contents = fs::read_to_string(&output_path).expect("output file");
    assert!(contents.ends_with('\n'));
    assert_eq!(
        syntax_contract_file_check(&output_path),
        Ok(SyntaxContractArtifactCheck::Match {
            fingerprint: syntax_contract_command_output(SyntaxContractOutputMode::Fingerprint)
                .expect("fingerprint")
        })
    );
    assert_eq!(run(&args(&["--check"])), ExitCode::from(2));

    let _ = fs::remove_file(&output_path);
}
