use super::{
    VmIoDiagnostic, VmIoDiagnosticLog, VmIoDiagnosticResource, VmIoDiagnosticResourceKind,
    VmIoDiagnosticSeverity, VmIoDiagnosticSourceMap, VmIoDiagnosticSpan,
    VM_IO_RESOURCE_REMOVED_WITHOUT_DESELECTING,
};

/// Verifies VM I/O diagnostics retain source-map identity.
#[test]
fn io_diagnostics_render_source_map_aware_runtime_failures() {
    let source_map = VmIoDiagnosticSourceMap::new(
        "app.Main:checksum-1",
        "app.Main",
        "serve",
        "src/app/Main.terl",
        VmIoDiagnosticSpan::new(12, 5, 12, 19),
    )
    .expect("source map");
    let resource =
        VmIoDiagnosticResource::new(VmIoDiagnosticResourceKind::TcpStream, "tcp:stream:4")
            .expect("resource");
    let diagnostic = VmIoDiagnostic::new(
        "io.backpressure",
        "stream inbox is full",
        VmIoDiagnosticSeverity::Warning,
        "tcp.receive",
        resource,
        source_map,
    )
    .expect("diagnostic");

    let mut log = VmIoDiagnosticLog::new();
    log.record_diagnostic(diagnostic.clone());

    assert_eq!(
        diagnostic.source_map.render_source_map_location(),
        "src/app/Main.terl:12:5-12:19 [app.Main::serve @ app.Main:checksum-1]"
    );
    assert!(diagnostic
        .render_text()
        .contains("Warning io.backpressure during tcp.receive"));
    assert_eq!(
        log.diagnostics_for_source_map("app.Main:checksum-1"),
        vec![diagnostic]
    );
    assert!(log.diagnostics_for_source_map("other").is_empty());
}

/// Verifies malformed source-map-aware I/O diagnostics are rejected.
#[test]
fn io_diagnostics_reject_malformed_source_map_context() {
    assert_eq!(
        VmIoDiagnosticSourceMap::new(
            "",
            "app.Main",
            "serve",
            "src/app/Main.terl",
            VmIoDiagnosticSpan::new(1, 1, 1, 1),
        )
        .expect_err("empty source map id"),
        "VM I/O diagnostic source_map_id cannot be empty"
    );
    assert_eq!(
        VmIoDiagnosticSourceMap::new(
            "source-map",
            "app.Main",
            "serve",
            "src/app/Main.terl",
            VmIoDiagnosticSpan::new(2, 5, 1, 1),
        )
        .expect_err("backwards span"),
        "VM I/O diagnostic source span cannot move backwards"
    );
    let source_map = VmIoDiagnosticSourceMap::new(
        "source-map",
        "app.Main",
        "serve",
        "src/app/Main.terl",
        VmIoDiagnosticSpan::new(1, 1, 1, 1),
    )
    .expect("source map");
    let resource =
        VmIoDiagnosticResource::new(VmIoDiagnosticResourceKind::PackageDownload, "pkg:1")
            .expect("resource");
    assert_eq!(
        VmIoDiagnostic::new(
            "",
            "download failed",
            VmIoDiagnosticSeverity::Error,
            "package.download",
            resource,
            source_map,
        )
        .expect_err("empty code"),
        "VM I/O diagnostic code cannot be empty"
    );
    assert_eq!(
        VmIoDiagnosticResource::new(VmIoDiagnosticResourceKind::UdpSocket, "")
            .expect_err("empty resource handle"),
        "VM I/O diagnostic resource handle cannot be empty"
    );
}

#[test]
fn diagnostic_probe_latches_only_post_install_typed_resource_fault() {
    let mut log = VmIoDiagnosticLog::new();
    log.record_diagnostic(diagnostic(
        VM_IO_RESOURCE_REMOVED_WITHOUT_DESELECTING,
        "historical resource fault",
    ));
    let probe = log
        .install_probe(VM_IO_RESOURCE_REMOVED_WITHOUT_DESELECTING)
        .expect("install typed probe");
    assert!(!probe.matched(&log).expect("empty probe query"));

    log.record_diagnostic(diagnostic(
        "vm.io.ready_callback_failure",
        "driver gone away without deselecting",
    ));
    assert!(
        !probe.matched(&log).expect("lookalike message query"),
        "legacy phrase matching must not replace typed diagnostic identity"
    );

    log.record_diagnostic(diagnostic(
        VM_IO_RESOURCE_REMOVED_WITHOUT_DESELECTING,
        "resource interest remained registered after removal",
    ));
    assert!(probe.matched(&log).expect("typed fault query"));
    log.record_diagnostic(diagnostic("vm.io.other", "unrelated later event"));
    assert!(
        probe.matched(&log).expect("sticky typed fault query"),
        "a matched probe must stay matched while its log is append-only"
    );
}

#[test]
fn diagnostic_probe_enforces_log_identity_and_close_lifecycle() {
    let log = VmIoDiagnosticLog::new();
    let other_log = VmIoDiagnosticLog::new();
    assert_eq!(
        log.install_probe("   ").expect_err("blank probe code"),
        "VM I/O diagnostic probe code cannot be empty"
    );

    let mut probe = log
        .install_probe(VM_IO_RESOURCE_REMOVED_WITHOUT_DESELECTING)
        .expect("install typed probe");
    assert_eq!(
        probe.matched(&other_log).expect_err("cross-log query"),
        "VM I/O diagnostic probe belongs to a different log"
    );
    assert_eq!(
        probe.close(&other_log).expect_err("cross-log close"),
        "VM I/O diagnostic probe belongs to a different log"
    );
    probe.close(&log).expect("close owning probe");
    assert_eq!(
        probe.matched(&log).expect_err("closed query"),
        "VM I/O diagnostic probe is closed"
    );
    assert_eq!(
        probe.close(&log).expect_err("duplicate close"),
        "VM I/O diagnostic probe is already closed"
    );
}

fn diagnostic(code: &str, message: &str) -> VmIoDiagnostic {
    let source_map = VmIoDiagnosticSourceMap::new(
        "runtime:io-probe",
        "runtime.Io",
        "observe",
        "runtime/io",
        VmIoDiagnosticSpan::new(1, 1, 1, 1),
    )
    .expect("diagnostic source map");
    let resource =
        VmIoDiagnosticResource::new(VmIoDiagnosticResourceKind::TcpStream, "tcp:stream:probe")
            .expect("diagnostic resource");
    VmIoDiagnostic::new(
        code,
        message,
        VmIoDiagnosticSeverity::Error,
        "io.resource.cleanup",
        resource,
        source_map,
    )
    .expect("diagnostic")
}
