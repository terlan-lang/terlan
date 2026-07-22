//! Tests for native generation reference accounting.

use super::*;

/// Verifies reference snapshots omit zeroes and render in canonical class order.
#[test]
fn generation_reference_snapshot_proves_quiescence_and_orders_diagnostics() {
    let mut snapshot = VmNativeGenerationReferenceSnapshot::new();
    assert!(snapshot.is_quiescent());
    assert_eq!(snapshot.total(), 0);
    assert_eq!(snapshot.render_pending(), "none");

    snapshot.record(VmNativeGenerationReferenceClass::Timer, 2);
    snapshot.record(VmNativeGenerationReferenceClass::NativeFrame, 1);
    snapshot.add(VmNativeGenerationReferenceClass::Timer, 3);
    snapshot.record(VmNativeGenerationReferenceClass::Debugger, 0);

    assert_eq!(snapshot.total(), 6);
    assert_eq!(snapshot.count(VmNativeGenerationReferenceClass::Timer), 5);
    assert_eq!(snapshot.render_pending(), "native_frames=1,timers=5");
    snapshot.record(VmNativeGenerationReferenceClass::NativeFrame, 0);
    snapshot.record(VmNativeGenerationReferenceClass::Timer, 0);
    assert!(snapshot.is_quiescent());
}
