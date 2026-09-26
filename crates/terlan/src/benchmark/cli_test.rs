use super::*;

#[test]
fn retired_http_baselines_are_rejected() {
    for command in ["native-boundary-http-baseline", "vm-http-runtime-baseline"] {
        assert_eq!(run_command(Some(command)), ExitCode::from(2));
    }
}

#[test]
fn vm_report_describes_native_artifacts_without_source_interpretation() {
    for stack in [
        VmRuntimeStack::resolved(Path::new("terlan-vm"), Path::new("terlc")),
        VmRuntimeStack::unresolved(),
    ] {
        let value = serde_json::to_value(stack).expect("serialize runtime metadata");
        assert!(value.get("source_execution").is_none());
        assert!(value.get("skipped_track_policy").is_none());
        assert!(value["artifact_execution"]
            .as_str()
            .expect("artifact execution")
            .contains("terlan-vm load <application.tvm>"));
    }
}
