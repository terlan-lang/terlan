use std::path::{Path, PathBuf};

use crate::{FuzzProfile, Oracle, ProfileRequest, RunOptions};

/// Validated operation requested by the `terlc fuzz` command line.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Types(RunOptions),
    Replay(PathBuf),
    Reduce(PathBuf),
    CorpusCheck(PathBuf),
    Profile(ProfileRequest),
    VerifyResult(PathBuf),
}

/// Parses a type-generation or replay request without executing it.
pub fn parse(args: &[String]) -> Result<Action, String> {
    match args.first().map(String::as_str) {
        Some("types") => parse_types(&args[1..]).map(Action::Types),
        Some("replay") if args.len() == 2 => Ok(Action::Replay(PathBuf::from(&args[1]))),
        Some("replay") => Err("usage: terlc fuzz replay <case-dir>".into()),
        Some("reduce") if args.len() == 2 => Ok(Action::Reduce(PathBuf::from(&args[1]))),
        Some("reduce") => Err("usage: terlc fuzz reduce <case-dir>".into()),
        Some("corpus") if args.get(1).map(String::as_str) == Some("check") && args.len() == 3 => {
            Ok(Action::CorpusCheck(PathBuf::from(&args[2])))
        }
        Some("corpus") => Err("usage: terlc fuzz corpus check <corpus-dir>".into()),
        Some("profile") => parse_profile(&args[1..]).map(Action::Profile),
        Some("result")
            if args.get(1).map(String::as_str) == Some("verify") && args.len() == 3 =>
        {
            Ok(Action::VerifyResult(PathBuf::from(&args[2])))
        }
        Some("result") => Err("usage: terlc fuzz result verify <result.json>".into()),
        _ => Err("expected `types`, `replay`, `reduce`, `corpus`, `profile`, or `result`; run `terlc help fuzz`".into()),
    }
}

/// Applies a command-adapter output directory to generated cases.
pub fn apply_output_directory(action: Action, out_dir: Option<&Path>) -> Action {
    match (action, out_dir) {
        (Action::Types(mut options), Some(path)) => {
            options.out_dir = path.to_path_buf();
            Action::Types(options)
        }
        (Action::Profile(mut request), Some(path)) => {
            request.out_dir = path.to_path_buf();
            Action::Profile(request)
        }
        (other, _) => other,
    }
}

fn parse_profile(args: &[String]) -> Result<ProfileRequest, String> {
    let profile = args
        .first()
        .ok_or_else(|| "usage: terlc fuzz profile <smoke|candidate|scheduled|impacted>".to_string())
        .and_then(|value| FuzzProfile::parse(value))?;
    let mut request = ProfileRequest {
        profile,
        out_dir: PathBuf::from("target/fuzz/profile"),
        rotation: 0,
        changed_paths: Vec::new(),
    };
    let mut index = 1;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("{flag} requires a value"))?;
        match flag {
            "--out-dir" => request.out_dir = PathBuf::from(value),
            "--rotation" => request.rotation = number(value, flag)?,
            "--changed-path" => request.changed_paths.push(value.clone()),
            _ => return Err(format!("unknown fuzz profile option `{flag}`")),
        }
        index += 2;
    }
    request.validate()?;
    Ok(request)
}

fn parse_types(args: &[String]) -> Result<RunOptions, String> {
    let mut options = RunOptions::default();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("{flag} requires a value"))?;
        match flag {
            "--seed" => options.seed = number(value, flag)?,
            "--cases" => options.cases = number(value, flag)?,
            "--max-depth" => options.max_depth = number(value, flag)?,
            "--oracle" => options.oracle = Oracle::parse(value)?,
            "--family" => options.families.push(value.clone()),
            "--out-dir" => options.out_dir = PathBuf::from(value),
            "--case-timeout-ms" => options.case_timeout_ms = number(value, flag)?,
            "--total-timeout-ms" => options.total_timeout_ms = number(value, flag)?,
            "--rss-limit-mib" => {
                let mib: u64 = number(value, flag)?;
                options.rss_limit_bytes = mib
                    .checked_mul(1024 * 1024)
                    .ok_or_else(|| "--rss-limit-mib is too large".to_string())?;
            }
            "--cpu-limit-ms" => options.cpu_limit_ms = number(value, flag)?,
            _ => return Err(format!("unknown fuzz option `{flag}`")),
        }
        index += 2;
    }
    options.validate()?;
    Ok(options)
}

fn number<T>(text: &str, flag: &str) -> Result<T, String>
where
    T: std::str::FromStr,
{
    text.parse()
        .map_err(|_| format!("{flag} requires a positive integer"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn parses_every_type_run_option() {
        let args = strings(&[
            "types",
            "--seed",
            "17",
            "--cases",
            "9",
            "--max-depth",
            "4",
            "--oracle",
            "none",
            "--family",
            "record",
            "--family",
            "generic_field_receiver",
            "--out-dir",
            "cases",
            "--case-timeout-ms",
            "3",
            "--total-timeout-ms",
            "4",
            "--rss-limit-mib",
            "5",
            "--cpu-limit-ms",
            "6",
        ]);
        let expected = RunOptions {
            seed: 17,
            cases: 9,
            max_depth: 4,
            oracle: Oracle::None,
            families: vec!["record".into(), "generic_field_receiver".into()],
            out_dir: PathBuf::from("cases"),
            case_timeout_ms: 3,
            total_timeout_ms: 4,
            rss_limit_bytes: 5 * 1024 * 1024,
            cpu_limit_ms: 6,
        };
        assert_eq!(parse(&args), Ok(Action::Types(expected)));
    }

    #[test]
    fn parses_replay() {
        assert_eq!(
            parse(&strings(&["replay", "case"])),
            Ok(Action::Replay(PathBuf::from("case")))
        );
        assert_eq!(
            parse(&strings(&["reduce", "case"])),
            Ok(Action::Reduce(PathBuf::from("case")))
        );
        assert_eq!(
            parse(&strings(&["corpus", "check", "cases"])),
            Ok(Action::CorpusCheck(PathBuf::from("cases")))
        );
        assert_eq!(
            parse(&strings(&[
                "profile",
                "scheduled",
                "--rotation",
                "17",
                "--out-dir",
                "scheduled",
            ])),
            Ok(Action::Profile(ProfileRequest {
                profile: FuzzProfile::Scheduled,
                out_dir: PathBuf::from("scheduled"),
                rotation: 17,
                changed_paths: Vec::new(),
            }))
        );
        assert_eq!(
            parse(&strings(&[
                "profile",
                "impacted",
                "--changed-path",
                "one",
                "--changed-path",
                "two",
            ])),
            Ok(Action::Profile(ProfileRequest {
                profile: FuzzProfile::Impacted,
                out_dir: PathBuf::from("target/fuzz/profile"),
                rotation: 0,
                changed_paths: vec!["one".into(), "two".into()],
            }))
        );
        assert_eq!(
            parse(&strings(&["result", "verify", "result.json"])),
            Ok(Action::VerifyResult(PathBuf::from("result.json")))
        );
    }

    #[test]
    fn output_override_only_changes_generated_cases() {
        let generated = parse(&strings(&["types"])).expect("type fuzz action");
        let expected = RunOptions {
            out_dir: PathBuf::from("target/fuzz/custom"),
            ..RunOptions::default()
        };
        assert_eq!(
            apply_output_directory(generated, Some(Path::new("target/fuzz/custom"))),
            Action::Types(expected)
        );

        let default = parse(&strings(&["types"])).expect("type fuzz action");
        assert_eq!(
            apply_output_directory(default, None),
            parse(&strings(&["types"])).expect("default action")
        );
        let replay = Action::Replay(PathBuf::from("case"));
        assert_eq!(
            apply_output_directory(replay, Some(Path::new("ignored"))),
            Action::Replay(PathBuf::from("case"))
        );
        let reduce = Action::Reduce(PathBuf::from("case"));
        assert_eq!(
            apply_output_directory(reduce, Some(Path::new("ignored"))),
            Action::Reduce(PathBuf::from("case"))
        );
        let corpus = Action::CorpusCheck(PathBuf::from("corpus"));
        assert_eq!(
            apply_output_directory(corpus, Some(Path::new("ignored"))),
            Action::CorpusCheck(PathBuf::from("corpus"))
        );
        let profile = Action::Profile(ProfileRequest {
            profile: FuzzProfile::Smoke,
            out_dir: PathBuf::from("old"),
            rotation: 0,
            changed_paths: Vec::new(),
        });
        assert_eq!(
            apply_output_directory(profile, Some(Path::new("new"))),
            Action::Profile(ProfileRequest {
                profile: FuzzProfile::Smoke,
                out_dir: PathBuf::from("new"),
                rotation: 0,
                changed_paths: Vec::new(),
            })
        );
        let result = Action::VerifyResult(PathBuf::from("result.json"));
        assert_eq!(
            apply_output_directory(result, Some(Path::new("ignored"))),
            Action::VerifyResult(PathBuf::from("result.json"))
        );
    }

    #[test]
    fn rejects_invalid_command_shapes() {
        for (args, expected) in [
            (vec![], "expected"),
            (strings(&["unknown"]), "expected"),
            (strings(&["replay"]), "usage"),
            (strings(&["replay", "one", "two"]), "usage"),
            (strings(&["reduce"]), "usage"),
            (strings(&["reduce", "one", "two"]), "usage"),
            (strings(&["corpus"]), "usage"),
            (strings(&["corpus", "other", "cases"]), "usage"),
            (strings(&["corpus", "check"]), "usage"),
            (strings(&["corpus", "check", "one", "two"]), "usage"),
            (strings(&["profile"]), "usage"),
            (strings(&["profile", "full"]), "fuzz profile"),
            (
                strings(&["profile", "impacted"]),
                "requires at least one --changed-path",
            ),
            (
                strings(&["profile", "smoke", "--rotation"]),
                "requires a value",
            ),
            (
                strings(&["profile", "smoke", "--rotation", "bad"]),
                "positive integer",
            ),
            (
                strings(&["profile", "smoke", "--unknown", "x"]),
                "unknown fuzz profile option",
            ),
            (strings(&["result"]), "usage"),
            (strings(&["result", "verify"]), "usage"),
            (strings(&["result", "other", "result.json"]), "usage"),
            (strings(&["types", "--seed"]), "requires a value"),
            (strings(&["types", "--seed", "no"]), "positive integer"),
            (strings(&["types", "--surprise", "1"]), "unknown"),
            (
                strings(&["types", "--family", "unknown"]),
                "unknown fuzz family",
            ),
            (
                strings(&["types", "--family", "record", "--family", "record"]),
                "selected more than once",
            ),
        ] {
            assert!(parse(&args).expect_err("must reject").contains(expected));
        }
    }

    #[test]
    fn rejects_overflowing_rss_mebibytes() {
        let value = (u64::MAX / (1024 * 1024) + 1).to_string();
        let args = ["types".to_string(), "--rss-limit-mib".to_string(), value];
        assert_eq!(
            parse(&args).expect_err("overflow"),
            "--rss-limit-mib is too large"
        );
    }

    #[test]
    fn rejects_invalid_values_for_every_option_parser() {
        for flag in [
            "--cases",
            "--max-depth",
            "--case-timeout-ms",
            "--total-timeout-ms",
            "--rss-limit-mib",
            "--cpu-limit-ms",
        ] {
            let args = strings(&["types", flag, "not-a-number"]);
            assert!(parse(&args).expect_err("number").contains(flag));
        }
        assert!(parse(&strings(&["types", "--oracle", "execute"]))
            .expect_err("oracle")
            .contains("oracle"));
        assert!(parse(&strings(&["types", "--cases", "0"]))
            .expect_err("validation")
            .contains("cases"));
    }
}
