use std::fs;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::{
    evaluate_repl_prompt_inputs, parse_repl_command_args, repl_generation_run_name,
    run_repl_expression_in_session_with_output, ReplCompilerService, ReplExpressionRequest,
    ReplValueBinding,
};
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::{ColorChoice, DiagnosticFormat};

impl ReplCompilerService {
    /// Returns the active supervised native-image epoch for focused tests.
    fn active_native_epoch(&self) -> Option<u64> {
        let active = self.active.as_ref()?;
        active.shard.lifecycle_epoch().map(|epoch| epoch.as_u64())
    }

    /// Returns completed calls for the currently admitted generation.
    fn active_native_call_count(&self) -> Option<u64> {
        self.active
            .as_ref()
            .map(|active| active.completed_native_call_count())
    }
}

const REPL_P95_SAMPLE_COUNT: usize = 7;
const REPL_WARM_P95_BUDGET: Duration = Duration::from_secs(1);

#[test]
#[ignore = "run by make tvm-aot-compilation-time-check in release mode"]
fn native_repl_unchanged_generation_p95_stays_under_one_second() {
    let root = std::env::temp_dir().join(format!(
        "terlan-repl-aot-budget-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create native REPL budget root");
    let module_name = "repl_aot_budget";
    let declarations = Vec::<String>::new();
    let bindings = Vec::<ReplValueBinding>::new();
    let mut compiler_service = ReplCompilerService::default();

    run_scalar_generation(
        &mut compiler_service,
        "40 + 2",
        module_name,
        &declarations,
        &bindings,
        &root,
    );

    let mut unchanged = Vec::with_capacity(REPL_P95_SAMPLE_COUNT);
    for _ in 0..REPL_P95_SAMPLE_COUNT {
        let started = Instant::now();
        run_scalar_generation(
            &mut compiler_service,
            "40 + 2",
            module_name,
            &declarations,
            &bindings,
            &root,
        );
        unchanged.push(started.elapsed());
    }

    assert_p95_budget("unchanged", &mut unchanged);
    fs::remove_dir_all(&root).expect("remove native REPL budget root");
}

#[test]
fn scalar_repl_generation_executes_without_resident_core_ir() {
    let root = std::env::temp_dir().join(format!(
        "terlan-repl-aot-native-only-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create native-only REPL root");
    let mut compiler_service = ReplCompilerService::default();

    run_scalar_generation(
        &mut compiler_service,
        "40 + 2",
        "repl_native_only",
        &[],
        &[],
        &root,
    );

    fs::remove_dir_all(&root).expect("remove native-only REPL root");
}

#[test]
fn repl_debug_mode_executes_generation_through_live_vm_debugger() {
    let root = std::env::temp_dir().join(format!(
        "terlan-repl-debug-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create debugger REPL root");
    let mut compiler_service = ReplCompilerService::default();
    compiler_service.set_debug_enabled(true);
    let module_name = "repl_debug_live";
    let expression = "40 + 2";
    let run_name = repl_generation_run_name("repl_debug_eval", expression, &[], &[], module_name);
    let mut debug_events = Vec::new();
    let value = run_repl_expression_in_session_with_output(
        &mut compiler_service,
        None,
        ReplExpressionRequest {
            expression,
            declarations: &[],
            value_bindings: &[],
            module_name,
            run_name: &run_name,
            temp_dir: &root,
            diagnostic_format: DiagnosticFormat::Text {
                color: ColorChoice::Never,
            },
            native_policy: NativePolicy::NativeBoundaryOptional,
            target_profile: TargetProfile::Vm,
        },
        &mut |event| debug_events.push(event.to_string()),
    )
    .expect("execute REPL expression through debugger");

    assert_eq!(value, "42");
    assert!(debug_events
        .iter()
        .any(|event| event.contains("stopped:breakpoint:")));
    assert!(debug_events.iter().any(|event| event.contains("bt:")));
    fs::remove_dir_all(&root).expect("remove debugger REPL root");
}

/// Managed REPL results execute and render through the admitted native image.
#[test]
fn managed_repl_generation_executes_without_resident_core_ir() {
    let outputs = evaluate_repl_prompt_inputs(
        &["[1, 2].".to_string()],
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        NativePolicy::NativeBoundaryOptional,
        TargetProfile::Vm,
    )
    .expect("compile and execute managed native REPL generation");

    assert_eq!(outputs, vec![vec!["[1, 2]".to_string()]]);
}

/// Runtime selection is absent because every REPL expression is native AOT.
#[test]
fn repl_command_rejects_runtime_selection() {
    assert_eq!(
        parse_repl_command_args(&["--runtime".into(), "vm".into()])
            .expect_err("runtime selector must be removed"),
        "unknown repl option: --runtime"
    );
}

/// Unchanged source reuses the active admitted shard without replacing it.
#[test]
fn unchanged_repl_generation_reuses_active_native_shard() {
    let root = std::env::temp_dir().join(format!(
        "terlan-repl-aot-reuse-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create native REPL reuse root");
    let mut compiler_service = ReplCompilerService::default();

    run_scalar_generation(
        &mut compiler_service,
        "40 + 2",
        "repl_native_reuse",
        &[],
        &[],
        &root,
    );
    assert_eq!(compiler_service.active_native_epoch(), Some(1));
    assert_eq!(compiler_service.active_native_call_count(), Some(1));

    run_scalar_generation(
        &mut compiler_service,
        "40 + 2",
        "repl_native_reuse",
        &[],
        &[],
        &root,
    );
    assert_eq!(compiler_service.active_native_epoch(), Some(1));
    assert_eq!(compiler_service.active_native_call_count(), Some(2));

    fs::remove_dir_all(&root).expect("remove native REPL reuse root");
}

#[test]
fn float_repl_generation_executes_without_resident_core_ir() {
    let root = std::env::temp_dir().join(format!(
        "terlan-repl-aot-float-native-only-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create float native-only REPL root");
    let mut compiler_service = ReplCompilerService::default();
    let module_name = "repl_float_native_only";
    for (index, (expression, expected)) in [
        ("1.5 + 2.25", "3.75"),
        ("1 + 2.75", "3.75"),
        ("3.75 > 2", "true"),
        ("-1.25", "-1.25"),
    ]
    .into_iter()
    .enumerate()
    {
        let run_name =
            repl_generation_run_name("repl_float_eval", expression, &[], &[], module_name);
        let value = run_repl_expression_in_session_with_output(
            &mut compiler_service,
            None,
            ReplExpressionRequest {
                expression,
                declarations: &[],
                value_bindings: &[],
                module_name,
                run_name: &run_name,
                temp_dir: &root,
                diagnostic_format: DiagnosticFormat::Text {
                    color: ColorChoice::Never,
                },
                native_policy: NativePolicy::NativeBoundaryOptional,
                target_profile: TargetProfile::Vm,
            },
            &mut |_| {},
        )
        .unwrap_or_else(|error| panic!("native Float REPL generation failed: {error}"));

        assert_eq!(value, expected);
        assert_eq!(
            compiler_service.active_native_epoch(),
            Some(index as u64 + 1),
            "each changed native REPL image must replace the prior supervised epoch"
        );
    }
    fs::remove_dir_all(&root).expect("remove float native-only REPL root");
}

#[test]
#[ignore = "run by make tvm-aot-compilation-time-check in release mode"]
fn native_repl_changed_generation_p95_stays_under_one_second() {
    let root = std::env::temp_dir().join(format!(
        "terlan-repl-aot-budget-changed-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create native REPL budget root");
    let module_name = "repl_aot_budget";
    let declarations = Vec::<String>::new();
    let bindings = Vec::<ReplValueBinding>::new();
    let mut compiler_service = ReplCompilerService::default();

    run_scalar_generation(
        &mut compiler_service,
        "40 + 2",
        module_name,
        &declarations,
        &bindings,
        &root,
    );

    let mut changed = Vec::with_capacity(REPL_P95_SAMPLE_COUNT);
    for index in 0..REPL_P95_SAMPLE_COUNT {
        let expression = format!("{} + {}", 40 + index, index);
        let started = Instant::now();
        run_scalar_generation(
            &mut compiler_service,
            &expression,
            module_name,
            &declarations,
            &bindings,
            &root,
        );
        changed.push(started.elapsed());
    }

    assert_p95_budget("changed", &mut changed);
    fs::remove_dir_all(&root).expect("remove native REPL budget root");
}

fn run_scalar_generation(
    compiler_service: &mut ReplCompilerService,
    expression: &str,
    module_name: &str,
    declarations: &[String],
    bindings: &[ReplValueBinding],
    root: &std::path::Path,
) {
    let run_name = repl_generation_run_name(
        "repl_budget_eval",
        expression,
        declarations,
        bindings,
        module_name,
    );
    let value = run_repl_expression_in_session_with_output(
        compiler_service,
        None,
        ReplExpressionRequest {
            expression,
            declarations,
            value_bindings: bindings,
            module_name,
            run_name: &run_name,
            temp_dir: root,
            diagnostic_format: DiagnosticFormat::Text {
                color: ColorChoice::Never,
            },
            native_policy: NativePolicy::NativeBoundaryOptional,
            target_profile: TargetProfile::Vm,
        },
        &mut |_| {},
    )
    .unwrap_or_else(|error| panic!("native REPL generation `{expression}` failed: {error}"));
    assert!(!value.is_empty());
}

fn assert_p95_budget(label: &str, samples: &mut [Duration]) {
    samples.sort_unstable();
    let index = ((samples.len() * 95).div_ceil(100)).saturating_sub(1);
    let p95 = samples[index];
    assert!(
        p95 < REPL_WARM_P95_BUDGET,
        "{label} native REPL p95 {p95:?} exceeded {REPL_WARM_P95_BUDGET:?}; samples={samples:?}"
    );
}
