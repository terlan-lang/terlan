//! Explicit prebuilt-AOT acceptance tier for the production proof request callers.

use super::*;

const TOOLS: &str = "TERLAN_COVERAGE_AOT_TOOLS";

pub(super) fn prepare(root: &Path) {
    let Some(tools) = std::env::var_os(TOOLS).map(PathBuf::from) else {
        return;
    };
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let compiler = tools.join("terlc");
    let vm = tools.join("terlan-vm");
    assert!(
        compiler.is_file() && vm.is_file(),
        "prebuilt AOT tools required"
    );
    let package = "scripts/self_validation/proof_release_evidence";
    write(root, "target/aot-probes/semantic-package/terlan.toml",
        "[package]\nname = \"proof-release-evidence\"\nversion = \"0.0.0\"\n[build]\nsource_roots = [\"src\"]\nartifact = \"library\"\n");
    for entry in fs::read_dir(repository.join(package).join("src/proof_release_evidence")).unwrap()
    {
        let entry = entry.unwrap();
        assert!(entry.file_type().unwrap().is_file());
        write(
            root,
            &format!(
                "target/aot-probes/semantic-package/src/proof_release_evidence/{}",
                entry.file_name().to_str().unwrap()
            ),
            fs::read(entry.path()).unwrap(),
        );
    }
    let mut make = fs::read_to_string(root.join("Makefile")).unwrap();
    for (name, path) in [
        (
            "Native",
            "scripts/self_validation/LeanProofNativeBoundaryTest.terl",
        ),
        ("Smoke", "scripts/self_validation/LeanProofSmokeTest.terl"),
        (
            "Semantic",
            "scripts/self_validation/proof_release_evidence/scripts/SemanticKernels.terls",
        ),
    ] {
        let source = fs::read_to_string(repository.join(path)).unwrap();
        // Keep the complete production function definitions. Only the entry point
        // selects the tiny fixture's real completed test instead of repository proofs.
        let probe = if name == "Semantic" {
            let entry = source.find("entry(): Bool ->").unwrap();
            let invoke = source[entry..].find("assert(entry());").unwrap() + entry;
            format!(
                "{}{}{}",
                &source[..entry],
                r#"entry(): Bool ->
                let selected = case Arguments.get(0) { Some(value) -> value; None -> "missing" };
                let runtime = ["cargo", "test", "--locked", "-p", "terlan", "--lib", selected, "--", "--exact"];
                let checked = request_rust_coverage(root(), runtime);
                checked.valid.

"#,
                &source[invoke..]
            )
        } else {
            let mut probe = source.replace(
                "import std.core.Option.{None, Some}.",
                "import std.system.Arguments.\nimport std.core.Option.{None, Some}.",
            );
            probe.push_str(r#"
pub main(): Bool ->
let repository = Environment.current_directory();
let selector = case Arguments.get(0) { Some(value) -> value; None -> "missing" };
case Process.run(rust_oracle_command(repository, selector)) {
    Ok(output) ->
        let _diagnostic = println(Process.stderr(output));
        rust_oracle_output_holds(Process.status(output), Process.stdout(output), rust_coverage_requested());
    Err(reason) -> let _diagnostic = println(Process.error_message(reason)); false
}.
"#);
            probe
        };
        let source_path = if name == "Semantic" {
            "target/aot-probes/semantic-package/scripts/Semantic.terls".to_owned()
        } else {
            format!("target/aot-probes/{name}.terl")
        };
        write(root, &source_path, probe);
        let output = format!("target/aot-probes/{name}");
        let mut build = command(root, &compiler);
        build.args([
            "build",
            "--incremental",
            &source_path,
            "--target",
            "terlan-vm",
            "--out-dir",
            &output,
        ]);
        assert!(
            ProcessControl::new(Duration::from_secs(300))
                .run(&mut build, |_| Ok(()))
                .is_ok(),
            "build real {name} probe"
        );
        let images = fs::read_dir(root.join(&output).join("vm"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|value| value == "tvm"))
            .collect::<Vec<_>>();
        assert_eq!(images.len(), 1);
        for (suffix, selector, prefix) in [
            ("pass", "normal", ""),
            ("missing", "missing_test", ""),
            (
                "changed",
                "normal",
                "TERLAN_TEST_SUSPICIOUS_OPTION=changed ",
            ),
        ] {
            let evaluation = if name == "Semantic" {
                "--script-eval"
            } else {
                "--entry main --test-eval"
            };
            make.push_str(&format!(
                "\naot-{name}-{suffix}:\n\t{prefix}TERLAN_LEAN_PROOF_ROOT=$(CURDIR) TERLAN_SEMANTIC_KERNEL_ROOT=$(CURDIR) \"{}\" run \"{}\" {evaluation} -- {selector}\n",
                vm.display(), images[0].display()
            ));
        }
    }
    write(root, "Makefile", make);
    assert!(run(command(
        root,
        env!("CARGO_BIN_EXE_terlan-test-orchestrator")
    )
    .args([
        "--install-snapshot",
        "target/validation-tools/terlan-test-orchestrator"
    ])));
}

/// Exercises real compiled callers; coverage comes from the already completed suite.
pub(super) fn exercise(root: &Path) {
    if std::env::var_os(TOOLS).is_none() {
        return;
    }
    let before = fs::read(root.join("target/bodies.txt")).unwrap();
    for name in ["Native", "Smoke", "Semantic"] {
        for (suffix, expected) in [("pass", true), ("missing", false), ("changed", false)] {
            let goal = format!("aot-{name}-{suffix}");
            let actual = run(command(root, root.join("target/driver")).args([
                "--with-hosted-cargo-coverage",
                "--",
                "make",
                "--no-print-directory",
                &goal,
            ]));
            assert_eq!(actual, expected, "compiled {goal}");
            let report: Value = serde_json::from_slice(
                &fs::read(root.join("target/quality/hosted-source-coverage.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(
                report["decision"] == "hosted-source-gates-covered",
                expected
            );
            assert_eq!(
                fs::read(root.join("target/bodies.txt")).unwrap(),
                before,
                "AOT caller replayed tests"
            );
        }
    }
    println!("[aot-proof-coverage] three compiled production callers passed; missing selections and changed environments rejected; zero test-body replay");
}
