//! JavaScript execution proof for exact nested lexical binding identities.

use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::formal_pipeline::compile_syntax_module_through_phases_with_profile;
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::DiagnosticFormat;

#[test]
fn javascript_preserves_outer_and_nested_binding_identities() {
    let source = r#"
module js_binding_identity.

pub choose(value: Int, pair: {Int, Int}): Int ->
    case pair {
        {value, _} -> value;
        _ -> value
    }.
"#;
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_binding_identity.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile JavaScript binding fixture");
    artifacts
        .core
        .binding_identities
        .validate()
        .expect("JavaScript CoreIR binding evidence");
    assert_eq!(
        artifacts
            .core
            .binding_identities
            .bindings
            .iter()
            .filter(|binding| binding.name == "value")
            .map(|binding| binding.id)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        2
    );
    let js = super::oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("emit JavaScript binding fixture");

    let root = std::env::temp_dir().join(format!(
        "terlan-js-binding-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create JavaScript binding fixture");
    let module_path = root.join("binding.mjs");
    let runner_path = root.join("runner.mjs");
    fs::write(&module_path, js).expect("write JavaScript binding module");
    fs::write(
        &runner_path,
        r#"
import { choose } from "./binding.mjs";
if (choose(17, false) !== 17) throw new Error("outer binding changed");
if (choose(17, [5, 9]) !== 5) throw new Error("nested binding not selected");
"#,
    )
    .expect("write JavaScript binding runner");
    let run = match Command::new("node").arg(&runner_path).output() {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::remove_dir_all(root).expect("remove JavaScript binding fixture");
            return;
        }
        Err(error) => panic!("run JavaScript binding proof: {error}"),
    };
    assert!(
        run.status.success(),
        "JavaScript binding proof failed:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    fs::remove_dir_all(root).expect("remove JavaScript binding fixture");
}
