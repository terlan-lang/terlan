use super::super::process::VmProcessId;
use super::{
    VmSupportBundleReplayExpectation, VmSupportBundleReplayRecorder, VmSupportBundleReplayResource,
    VmSupportBundleReplayResourceKind, VmSupportBundleReplaySource,
};
use crate::runtime::vm::multicore_replay::{VmMulticoreReplayEvidence, VmMulticoreReplayRecorder};
use crate::runtime::vm::native_image_diagnostics::{
    VmNativeGenerationReferenceClass, VmNativeGenerationReferenceSnapshot,
    VmNativeImageDiagnosticMetadata,
};
use crate::runtime::vm::scheduler_topology::VmSchedulerTopology;

/// Verifies replay metadata binds one exact admitted native generation.
#[test]
fn support_bundle_replay_metadata_binds_native_generation_once() {
    let mut references = VmNativeGenerationReferenceSnapshot::new();
    references.record(VmNativeGenerationReferenceClass::AsyncCapabilityCallback, 1);
    let native = VmNativeImageDiagnosticMetadata::new(
        "compiler:build:package:module",
        [7; 32],
        vec![12, 4],
        3,
        &references,
    )
    .expect("native diagnostics");
    let mut recorder = VmSupportBundleReplayRecorder::new(55);
    recorder
        .bind_native_image(native.clone())
        .expect("bind admitted image");
    assert!(recorder.bind_native_image(native.clone()).is_err());
    let metadata = recorder.finish_bundle();
    assert_eq!(metadata.native_image, Some(native));
    assert!(metadata
        .native_image
        .as_ref()
        .is_some_and(|image| image.generation_reference_total == 1));
}

/// Verifies native support JSON embeds bounded scheduler-local evidence.
#[test]
fn native_support_bundle_serializes_validated_multicore_evidence() {
    let native = VmNativeImageDiagnosticMetadata::new(
        "compiler:build:package:module",
        [9; 32],
        vec![3, 7],
        1,
        &VmNativeGenerationReferenceSnapshot::new(),
    )
    .expect("native diagnostics");
    let scheduler = VmSchedulerTopology::new(1)
        .expect("topology")
        .schedulers()
        .next()
        .expect("scheduler");
    let capture = VmMulticoreReplayRecorder::recording(scheduler, 8)
        .expect("recorder")
        .capture()
        .expect("capture");
    let evidence =
        VmMulticoreReplayEvidence::new(12, 1, 8, vec![capture]).expect("multicore evidence");
    let bundle = super::VmNativeSupportBundle::with_multicore_replay(native, evidence);
    let json =
        String::from_utf8(bundle.serialized_bytes().expect("support JSON")).expect("UTF-8 JSON");

    assert!(json.contains("\"multicoreReplay\""));
    assert!(json.contains("\"runtimeGeneration\": 12"));
    assert!(json.contains("\"replayable\": true"));
    assert!(!json.contains("SystemTime"));
    assert!(!json.contains("Instant"));
}

/// Verifies deterministic support-bundle replay metadata for VM I/O.
#[test]
fn support_bundle_replay_metadata_records_ordered_io_steps() {
    let process = VmProcessId::from_raw_for_test(41);
    let tcp = VmSupportBundleReplayResource::new(
        VmSupportBundleReplayResourceKind::TcpStream,
        "tcp:stream:1",
    )
    .expect("tcp resource");
    let udp =
        VmSupportBundleReplayResource::new(VmSupportBundleReplayResourceKind::UdpSocket, "udp:7")
            .expect("udp resource");
    let source = VmSupportBundleReplaySource::new("app.Main", "serve", 12, 5).expect("source");

    let mut recorder = VmSupportBundleReplayRecorder::new(99);
    let first = recorder
        .record_io_step_with_source(
            process,
            tcp.clone(),
            "tcp.receive.ready",
            "wake-reader",
            Some(source.clone()),
        )
        .expect("first step");
    let second = recorder
        .record_io_step(process, udp.clone(), "udp.packet.ready", "wake-receiver")
        .expect("second step");

    assert_eq!(first.sequence, 1);
    assert_eq!(first.source, Some(source));
    assert_eq!(second.sequence, 2);
    assert_eq!(recorder.replay_steps_after(1), vec![second.clone()]);

    let metadata = recorder.finish_bundle();
    assert_eq!(metadata.scheduler_seed, 99);
    assert!(metadata.finished);
    assert_eq!(metadata.steps, vec![first, second]);
    assert_eq!(
        recorder
            .record_io_step(process, tcp, "late.write", "reject")
            .expect_err("finished metadata is immutable"),
        "VM support-bundle replay metadata is finished"
    );
}

/// Verifies replay metadata rejects malformed and mismatched I/O identities.
#[test]
fn support_bundle_replay_metadata_rejects_mismatched_replay_identity() {
    let process = VmProcessId::from_raw_for_test(900);
    let resource = VmSupportBundleReplayResource::new(
        VmSupportBundleReplayResourceKind::PackageDownload,
        "package:download:3",
    )
    .expect("resource");
    let other_resource =
        VmSupportBundleReplayResource::new(VmSupportBundleReplayResourceKind::Timer, "timer:3")
            .expect("other resource");
    let mut recorder = VmSupportBundleReplayRecorder::new(123);
    recorder
        .record_io_step(
            process,
            resource.clone(),
            "package.chunk.ready",
            "wake-resolver",
        )
        .expect("record");

    let expected =
        VmSupportBundleReplayExpectation::new(1, process, resource.clone(), "package.chunk.ready")
            .expect("expectation");
    let matched = recorder
        .verify_replay_step(&expected)
        .expect("matching replay");
    assert_eq!(matched.outcome, "wake-resolver");

    let wrong_resource =
        VmSupportBundleReplayExpectation::new(1, process, other_resource, "package.chunk.ready")
            .expect("wrong resource");
    assert_eq!(
        recorder
            .verify_replay_step(&wrong_resource)
            .expect_err("resource mismatch"),
        "VM support-bundle replay step 1 did not match expected I/O identity"
    );

    let wrong_operation =
        VmSupportBundleReplayExpectation::new(1, process, resource, "package.complete")
            .expect("wrong operation");
    assert_eq!(
        recorder
            .verify_replay_step(&wrong_operation)
            .expect_err("operation mismatch"),
        "VM support-bundle replay step 1 did not match expected I/O identity"
    );

    let missing = VmSupportBundleReplayExpectation::new(
        2,
        process,
        wrong_operation.resource,
        "package.chunk.ready",
    )
    .expect("missing sequence");
    assert_eq!(
        recorder
            .verify_replay_step(&missing)
            .expect_err("missing sequence"),
        "VM support-bundle replay step 2 was not found"
    );
    assert_eq!(
        VmSupportBundleReplayResource::new(VmSupportBundleReplayResourceKind::WebSocket, "")
            .expect_err("empty handle"),
        "VM support-bundle resource handle cannot be empty"
    );
}
