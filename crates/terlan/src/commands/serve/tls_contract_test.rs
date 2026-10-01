use super::*;
use crate::commands::build::project_manifest::parse_project_manifest;

fn source(settings: &str) -> String {
    format!("[package]\nname = \"tls_test\"\nversion = \"0.0.9\"\n{settings}")
}

fn assert_admission(settings: &str, accepted: bool) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("terlan.toml");
    let source = source(settings);
    std::fs::write(&path, &source).unwrap();
    let build = parse_project_manifest(&source, &path).map(|manifest| manifest.server_tls);
    let runtime = read_runtime_server_tls(&path);
    assert_eq!(build.is_ok(), accepted, "build {settings}: {build:?}");
    assert_eq!(runtime.is_ok(), accepted, "runtime {settings}: {runtime:?}");
    if accepted {
        assert_eq!(build.unwrap(), runtime.unwrap());
    }
}

#[test]
fn build_and_runtime_share_package_tls_admission() {
    for settings in [
        "",
        "[server.tls]\nmode = \"auto\"\ndomains = [\"example.test\"]",
        "[server.tls]\nmode = \"auto\"\ndomains = [\"example.test\"]\nprimary_provider = \"letsencrypt\"\nfallback_provider = \"zerossl\"",
        "[server.tls]\nmode = \"manual\"\ncert = \"missing.pem\"\nkey = \"missing.key\"\npassphrase_env = \"NOT_SET\"",
        "[server.tls]\nmode = \"internal\"",
        "[server.tls]\nmode = \"internal\"\nserver_name = \"local\"\ntrust_local = false",
    ] {
        assert_admission(settings, true);
    }
    for settings in [
        "[server.tls]",
        "[server.tls]\nemail = \"user@example.test\"",
        "[server.tls]\nmode = \"other\"",
        "[server.tls]\nmode = \"auto\"",
        "[server.tls]\nmode = \"auto\"\ndomains = []",
        "[server.tls]\nmode = \"auto\"\ndomains = [\" \"]",
        "[server.tls]\nmode = \"auto\"\ndomains = [\"example.test\"]\ntrust_local = false",
        "[server.tls]\nmode = \"auto\"\ndomains = [\"example.test\"]\nprimary_provider = \"bad\"",
        "[server.tls]\nmode = \"manual\"\ncert = \"cert\"",
        "[server.tls]\nmode = \"manual\"\ncert = \"cert\"\nkey = \" \"",
        "[server.tls]\nmode = \"manual\"\ncert = \"cert\"\nkey = \"key\"\nfallback_provider = \"zerossl\"",
        "[server.tls]\nmode = \"internal\"\ndomains = []",
        "[server.tls]\nmode = \"internal\"\nserver_name = \"\"",
        "[server.tls]\nmode = \"internal\"\ntrust_local = \"false\"",
        "[server.tls]\nmode = \"internal\"\nextra = true",
    ] {
        assert_admission(settings, false);
    }
}

#[test]
fn runtime_manifest_failures_retain_host_path_and_operation_context() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("terlan.toml");
    let missing = read_runtime_server_tls(&path).unwrap_err().to_string();
    assert!(
        missing.contains("cannot read project manifest"),
        "{missing}"
    );
    assert!(missing.contains(path.to_str().unwrap()), "{missing}");
    for (source, expected) in [
        ("[server.tls", "cannot parse runtime TLS metadata"),
        (
            "[server.tls]\nmode = \"auto\"",
            "mode auto requires domains",
        ),
    ] {
        std::fs::write(&path, source).unwrap();
        let error = read_runtime_server_tls(&path).unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
        assert!(error.contains(path.to_str().unwrap()), "{error}");
    }
}
