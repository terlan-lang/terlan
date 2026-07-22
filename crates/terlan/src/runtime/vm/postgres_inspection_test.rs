use super::*;
use crate::terlan_native_boundary::request::RequestId;

#[test]
fn postgres_inspection_sanitizes_native_driver_wait_state() {
    let runtime = VmPostgresRuntime::new(1);
    let snapshot = runtime.inspection_snapshot(Some(VmPostgresDriverWait {
        request_id: RequestId { value: 404 },
        socket: 987_654_321,
        interest: VmPostgresIoInterest::Read,
    }));

    assert_eq!(
        snapshot.driver_wait,
        Some(VmPostgresDriverWaitSnapshot {
            request_id: 404,
            interest: VmPostgresIoInterest::Read,
        })
    );
    assert!(!format!("{snapshot:?}").contains("987654321"));
    assert!(snapshot.pending_requests.is_empty());
    assert!(snapshot.owners.is_empty());
}
