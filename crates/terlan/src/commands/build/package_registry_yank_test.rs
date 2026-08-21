use super::*;

#[test]
fn yank_arguments_require_mirror_package_and_version() {
    let args = [
        "yank",
        "--mirror",
        "mirror",
        "--package",
        "demo",
        "--version",
        "1.0.0",
        "--reason-class",
        "security",
        "--message",
        "superseded",
        "--replacement",
        "demo_next",
    ]
    .map(str::to_string);
    let parsed = parse_args(&args).unwrap();
    assert_eq!(parsed.mirror, PathBuf::from("mirror"));
    assert_eq!(parsed.package, "demo");
    assert_eq!(parsed.version, "1.0.0");
    assert_eq!(parsed.reason_class, YankReason::Security);
    assert_eq!(parsed.message, "superseded");
    assert_eq!(parsed.replacement.as_deref(), Some("demo_next"));
    assert!(parse_args(&["yank".into()]).is_err());
    assert!(parse_reason_class("unknown").is_err());
}
