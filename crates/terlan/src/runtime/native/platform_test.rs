use super::platform;

#[test]
fn current_platform_matches_the_compiled_rust_target() {
    let host = platform::current();
    assert_eq!(host.operating_system, std::env::consts::OS);
    assert_eq!(host.architecture, std::env::consts::ARCH);
    assert_eq!(host.path_separator, if cfg!(windows) { ";" } else { ":" });
    assert_eq!(
        host.executable_suffix,
        if cfg!(windows) { ".exe" } else { "" }
    );
}

#[test]
fn current_metrics_is_complete_or_explicitly_unavailable() {
    let metrics = platform::current_metrics();
    if metrics.available {
        assert!(metrics.message.is_empty());
        assert!(!metrics.kernel.is_empty());
        assert!(!metrics.operating_system.is_empty());
        assert!(!metrics.cpu_model.is_empty());
        assert!(metrics.memory_bytes > 0);
        assert!(metrics.available_memory_bytes > 0);
        assert!(!metrics.cpu_affinity.is_empty());
        assert!(!metrics.cpu_governor.is_empty());
        assert!(metrics.load_1m >= 0.0);
        assert!(metrics.load_5m >= 0.0);
        assert!(metrics.load_15m >= 0.0);
    } else {
        assert!(!metrics.message.is_empty());
    }
}
