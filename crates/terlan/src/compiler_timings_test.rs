#[test]
fn command_scope_enables_frontend_and_native_timings() {
    let scope = super::scope(true);
    assert!(super::frontend_enabled());
    assert!(super::native_enabled());
    drop(scope);
}
