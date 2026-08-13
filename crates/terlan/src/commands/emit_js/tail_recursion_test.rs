//! Executable stack-safety proofs for compiler-owned JavaScript tail loops.

use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::formal_pipeline::compile_syntax_module_through_phases_with_profile;
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::DiagnosticFormat;

#[test]
fn javascript_executes_one_million_direct_and_mutual_tail_calls_without_host_ptc() {
    let source = r#"
module js_tail_recursion.

pub direct(N: Int, Acc: Int): Int ->
    if { N == 0 -> Acc; true -> direct(N - 1, Acc + 1) }.

pub direct_let(N: Int, Acc: Int): Int ->
    if {
        N == 0 -> Acc;
        true -> let next = Acc + 1; direct_let(N - 1, next)
    }.

pub direct_case(N: Int, Acc: Int): Int ->
    case N {
        0 -> Acc;
        _ -> direct_case(N - 1, Acc + 1)
    }.

pub even(N: Int, Acc: Int): Int ->
    if { N == 0 -> Acc; true -> odd(N - 1, Acc + 1) }.

odd(N: Int, Acc: Int): Int ->
    if { N == 0 -> Acc; true -> even(N - 1, Acc + 1) }.

pub carry_tuple(N: Int, Value: {Int, Int}): {Int, Int} ->
    if { N == 0 -> Value; true -> carry_tuple(N - 1, Value) }.

pub carry_list(N: Int, Values: List[Int]): List[Int] ->
    if { N == 0 -> Values; true -> carry_list(N - 1, Values) }.

pub non_tail(N: Int): Int ->
    if { N == 0 -> 0; true -> non_tail(N - 1) + 1 }.

pub fail_after(N: Int, Acc: Int): Int ->
    if { N == 0 -> Acc div 0; true -> fail_after(N - 1, Acc + 1) }.
"#;
    let artifacts = compile_syntax_module_through_phases_with_profile(
        "js_tail_recursion.terl",
        source,
        DiagnosticFormat::default(),
        None,
        NativePolicy::default(),
        TargetProfile::JsShared,
    )
    .expect("compile JavaScript tail-recursion source");
    let js = super::oxc_backend::emit_core_module_with_oxc_codegen(&artifacts.core)
        .expect("emit stack-safe JavaScript");
    assert!(js.contains("while (true)"), "{js}");
    assert!(js.contains("__terlan_tail_component_"), "{js}");
    let non_tail = js
        .split("export function non_tail")
        .nth(1)
        .and_then(|tail| tail.split("\n}").next())
        .expect("emitted non-tail function body");
    assert!(non_tail.contains("non_tail(N - 1)"), "{non_tail}");
    assert!(!non_tail.contains("__terlan_tail_component_"), "{non_tail}");

    let root = std::env::temp_dir().join(format!(
        "terlan-js-tail-recursion-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create JavaScript tail fixture");
    let module_path = root.join("tail.mjs");
    let runner_path = root.join("runner.mjs");
    fs::write(&module_path, js).expect("write JavaScript tail module");
    fs::write(
        &runner_path,
        r#"
import { carry_list, carry_tuple, direct, direct_case, direct_let, even, non_tail } from "./tail.mjs";
if (direct(1_000_000, 0) !== 1_000_000) throw new Error("direct tail result");
if (direct_let(1_000_000, 0) !== 1_000_000) throw new Error("terminal let result");
if (direct_case(1_000_000, 0) !== 1_000_000) throw new Error("terminal case result");
if (even(1_000_000, 0) !== 1_000_000) throw new Error("mutual tail result");
const tuple = [17, 25];
if (carry_tuple(1_000_000, tuple) !== tuple) throw new Error("tuple identity");
const list = [1, 2, 3];
if (carry_list(1_000_000, list) !== list) throw new Error("list identity");
if (non_tail(1_000) !== 1_000) throw new Error("non-tail result");
try {
  const module = await import("./tail.mjs");
  module.fail_after(1_000_000, 0);
  throw new Error("missing checked division failure");
} catch (error) {
  if (error.terlanStatus !== 4 || error.terlanCode !== "DIVISION_BY_ZERO") throw error;
}
"#,
    )
    .expect("write JavaScript tail runner");
    let run = match Command::new("node").arg(&runner_path).output() {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::remove_dir_all(root).expect("remove JavaScript tail fixture");
            return;
        }
        Err(error) => panic!("run JavaScript tail proof: {error}"),
    };
    assert!(
        run.status.success(),
        "JavaScript tail proof failed:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    fs::remove_dir_all(root).expect("remove JavaScript tail fixture");
}
