//! Root helper ownership is independent of which exit path returns control.

use super::*;
use crate::runtime::native_image::TvmBoundaryType;

fn request(operation: &str, args: Vec<ReplValue>) -> PureNativeCapabilityRequest {
    PureNativeCapabilityRequest {
        capability: "package-native".into(),
        operation: operation.into(),
        arguments: Vec::new(),
        package_arguments: Some(args),
        result_type: TvmBoundaryType::Unit,
    }
}

fn allocate(helpers: &mut VmPackageNativeHelpers, owner: u64) -> (ReplValue, ReplValue) {
    let store = helpers
        .call(
            owner,
            &request("std.vm.distributed_state.store", vec![]),
            &[],
        )
        .unwrap();
    let state = helpers
        .call(
            owner,
            &request(
                "std.vm.distributed_state.export_snapshot",
                vec![store.clone()],
            ),
            &[],
        )
        .unwrap();
    let checkpoint = helpers
        .call(
            owner,
            &request(
                "std.vm.distributed_storage.checkpoint",
                vec![
                    ReplValue::String("checkpoint".into()),
                    ReplValue::Int(1),
                    state,
                ],
            ),
            &[],
        )
        .unwrap();
    (store, checkpoint)
}

fn assert_revoked(helpers: &mut VmPackageNativeHelpers, store: ReplValue, checkpoint: ReplValue) {
    assert!(helpers
        .call(
            17,
            &request("std.vm.distributed_state.export_snapshot", vec![store]),
            &[]
        )
        .is_err());
    assert!(helpers
        .call(
            17,
            &request("std.vm.distributed_storage.restore", vec![checkpoint]),
            &[]
        )
        .is_err());
}

#[test]
fn root_cleanup_revokes_only_its_owner_on_success_and_error() {
    for fails in [false, true] {
        let mut helpers = VmPackageNativeHelpers::default();
        let (other_store, other_checkpoint) = allocate(&mut helpers, 18);
        let mut allocated = None;
        let result: VmRuntimeResult<()> = (|| {
            let resources = OwnerResources {
                helpers: &mut helpers,
                owner: VmProcessId::from_native_owner(17).unwrap(),
            };
            allocated = Some(allocate(resources.helpers, 17));
            if fails {
                return Err("test root failure after allocating checkpoints".into());
            }
            Ok(())
        })();
        assert_eq!(result.is_err(), fails);
        let (store, checkpoint) = allocated.unwrap();
        assert_revoked(&mut helpers, store, checkpoint);
        assert!(helpers
            .call(
                18,
                &request(
                    "std.vm.distributed_state.export_snapshot",
                    vec![other_store]
                ),
                &[]
            )
            .is_ok());
        assert!(helpers
            .call(
                18,
                &request("std.vm.distributed_storage.restore", vec![other_checkpoint]),
                &[]
            )
            .is_ok());
    }
}

#[test]
fn root_cleanup_revokes_resources_during_unwinding() {
    let mut helpers = VmPackageNativeHelpers::default();
    let (store, checkpoint) = allocate(&mut helpers, 17);
    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _resources = OwnerResources {
            helpers: &mut helpers,
            owner: VmProcessId::from_native_owner(17).unwrap(),
        };
        panic!("test root unwind");
    }));
    assert!(unwind.is_err());
    assert_revoked(&mut helpers, store, checkpoint);
}
