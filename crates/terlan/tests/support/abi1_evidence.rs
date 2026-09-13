//! Correctness tests always run; writing release evidence requires explicit provenance.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

pub struct Evidence {
    revision: String,
    path: PathBuf,
    target: Option<String>,
}

impl Evidence {
    pub fn from_environment(gate: &str) -> Option<Self> {
        Self::parse(gate, |name| env::var_os(name)).expect("valid ABI evidence configuration")
    }

    fn parse(gate: &str, get: impl Fn(&str) -> Option<OsString>) -> Result<Option<Self>, String> {
        let enabled = get("TERLAN_ABI1_EMIT_EVIDENCE");
        let revision = get("TERLAN_ABI1_REVISION");
        let output = get("TERLAN_ABI1_EVIDENCE_OUTPUT");
        let target = get("TERLAN_ABI1_TARGET_TRIPLE");
        let fragment = get("TERLAN_ABI1_TARGET_FRAGMENT");
        if [&enabled, &revision, &output, &target, &fragment]
            .iter()
            .all(|value| value.is_none())
        {
            return Ok(None);
        }
        if enabled.as_deref() != Some(std::ffi::OsStr::new("1")) {
            return Err("evidence requires TERLAN_ABI1_EMIT_EVIDENCE=1".into());
        }
        let revision = revision
            .and_then(|value| value.into_string().ok())
            .filter(|value| {
                !value.is_empty()
                    && value != "unknown"
                    && value
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
            })
            .ok_or("TERLAN_ABI1_REVISION is required and must identify the tested source")?;
        let (target, path) = if gate == "cross-target-conformance" {
            if output.is_some() {
                return Err(
                    "cross-target evidence requires a target fragment, not an envelope output"
                        .into(),
                );
            }
            let target = target
                .and_then(|value| value.into_string().ok())
                .filter(|value| !value.is_empty())
                .ok_or("TERLAN_ABI1_TARGET_TRIPLE is required")?;
            let fragment = fragment
                .filter(|value| !value.is_empty())
                .ok_or("TERLAN_ABI1_TARGET_FRAGMENT is required")?;
            (Some(target), PathBuf::from(fragment))
        } else {
            if target.is_some() || fragment.is_some() {
                return Err("target metadata is only valid for the cross-target probe".into());
            }
            let path = match output {
                Some(value) if value.is_empty() => {
                    return Err("TERLAN_ABI1_EVIDENCE_OUTPUT cannot be empty".into());
                }
                Some(value) => PathBuf::from(value),
                None => PathBuf::from(format!("target/abi1-evidence/{gate}.json")),
            };
            (None, path)
        };
        Ok(Some(Self {
            revision,
            path,
            target,
        }))
    }

    pub fn write(&self, gate: &str, runs: Vec<Value>) {
        write_json(
            &self.path,
            &json!({
                "schema": "terlan.abi1.gate-evidence.v1",
                "gate": gate,
                "abi_version": 1,
                "managed_layout_profile": 1,
                "status": "passed",
                "revision": self.revision,
                "runs": runs,
            }),
        );
    }

    pub fn write_target(&self, architecture: &str) {
        let target = self.target.as_deref().expect("declared target");
        assert_eq!(target.split('-').next(), Some(architecture));
        write_json(
            &self.path,
            &json!({
                "target": target,
                "architecture": architecture,
                "pointer_width": usize::BITS,
                "endian": if cfg!(target_endian = "little") { "little" } else { "big" },
                "failures": 0,
                "status": "passed",
            }),
        );
    }
}

fn write_json(path: &Path, document: &Value) {
    fs::create_dir_all(path.parent().expect("evidence parent")).expect("create evidence parent");
    let text = serde_json::to_string_pretty(document).expect("serialize ABI evidence");
    fs::write(path, format!("{text}\n")).expect("write ABI evidence");
}

#[cfg(test)]
mod tests {
    use super::Evidence;

    fn parse(gate: &str, entries: &[(&str, &str)]) -> Result<Option<Evidence>, String> {
        Evidence::parse(gate, |key| {
            entries
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).into())
        })
    }

    #[test]
    fn ordinary_correctness_runs_do_not_request_evidence() {
        for gate in [
            "continuous-fuzz",
            "tail-latency",
            "specialization-equivalence",
            "cross-target-conformance",
        ] {
            assert!(parse(gate, &[]).unwrap().is_none());
        }
    }

    #[test]
    fn partial_configuration_cannot_silently_disable_evidence() {
        for name in [
            "TERLAN_ABI1_REVISION",
            "TERLAN_ABI1_EVIDENCE_OUTPUT",
            "TERLAN_ABI1_TARGET_TRIPLE",
            "TERLAN_ABI1_TARGET_FRAGMENT",
        ] {
            assert!(parse("continuous-fuzz", &[(name, "candidate")]).is_err());
        }
        for mode in ["", "0", "true"] {
            assert!(parse("continuous-fuzz", &[("TERLAN_ABI1_EMIT_EVIDENCE", mode)]).is_err());
        }
    }

    #[test]
    fn evidence_requires_an_explicit_valid_revision() {
        let mut entries = vec![("TERLAN_ABI1_EMIT_EVIDENCE", "1")];
        assert!(parse("continuous-fuzz", &entries).is_err());
        for revision in ["", "unknown", " ", "candidate\n"] {
            entries.truncate(1);
            entries.push(("TERLAN_ABI1_REVISION", revision));
            assert!(parse("continuous-fuzz", &entries).is_err());
        }
    }

    #[test]
    fn measured_evidence_preserves_revision_and_output() {
        let evidence = parse(
            "continuous-fuzz",
            &[
                ("TERLAN_ABI1_EMIT_EVIDENCE", "1"),
                ("TERLAN_ABI1_REVISION", "candidate-123"),
                ("TERLAN_ABI1_EVIDENCE_OUTPUT", "reports/fuzz.json"),
            ],
        )
        .unwrap()
        .unwrap();
        assert_eq!(evidence.revision, "candidate-123");
        assert_eq!(evidence.path, std::path::Path::new("reports/fuzz.json"));
        assert!(evidence.target.is_none());
    }

    #[test]
    fn cross_target_evidence_requires_both_target_and_fragment() {
        let mut entries = vec![
            ("TERLAN_ABI1_EMIT_EVIDENCE", "1"),
            ("TERLAN_ABI1_REVISION", "candidate-123"),
        ];
        assert!(parse("cross-target-conformance", &entries).is_err());
        entries.push(("TERLAN_ABI1_TARGET_TRIPLE", "x86_64-unknown-linux-gnu"));
        assert!(parse("cross-target-conformance", &entries).is_err());
        entries.push(("TERLAN_ABI1_TARGET_FRAGMENT", "reports/x86_64.json"));
        let evidence = parse("cross-target-conformance", &entries)
            .unwrap()
            .unwrap();
        assert_eq!(evidence.target.as_deref(), Some("x86_64-unknown-linux-gnu"));
        assert!(parse("continuous-fuzz", &entries).is_err());
        entries.push(("TERLAN_ABI1_EVIDENCE_OUTPUT", "reports/invalid.json"));
        assert!(parse("cross-target-conformance", &entries).is_err());
    }
}
