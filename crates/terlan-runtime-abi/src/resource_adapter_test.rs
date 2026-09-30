use super::*;
use crate::NativeValue;
use std::cell::Cell;
use std::rc::Rc;

#[derive(Debug)]
struct Tracked {
    values: Vec<i64>,
    clones: Rc<Cell<usize>>,
}

impl Clone for Tracked {
    fn clone(&self) -> Self {
        self.clones.set(self.clones.get() + 1);
        Self {
            values: self.values.clone(),
            clones: self.clones.clone(),
        }
    }
}

const fn operation(
    name: &'static str,
    arity: usize,
    mutates: bool,
    returns_resource: bool,
) -> NativeResourceOperation {
    NativeResourceOperation {
        operation: name,
        arity,
        resource_type: "test.Tracked",
        returns_resource,
        result_error: None,
        mutates_receiver: mutates,
    }
}

const ADAPTER: NativeResourceAdapter<Tracked> = NativeResourceAdapter {
    operations: &[
        operation("read", 1, false, false),
        operation("copy", 1, false, true),
        operation("extend", 2, true, true),
        operation("fail", 1, false, false),
        operation("fail_mutation", 1, true, true),
        operation("bad_contract", 0, true, true),
    ],
    invoke: |operation, args| {
        let [NativeResourceValue::Resource(value)] = args else {
            return Err(NativeAdapterError::new("test.type", "resource required", 0));
        };
        match operation {
            "read" => Ok(NativeResourceValue::Value(value.values.clone().into())),
            "copy" => Ok(NativeResourceValue::Resource((*value).clone())),
            "fail" => Err(NativeAdapterError::new("test.failure", "read failed", 17)),
            _ => panic!("unexpected read callback"),
        }
    },
    mutate: |operation, receiver, args| {
        if operation == "fail_mutation" {
            return Err(NativeAdapterError::new("test.failure", "write failed", 19));
        }
        assert_eq!(operation, "extend");
        let mut args = args.into_iter();
        let Some(NativeResourceValue::Resource(value)) = args.next() else {
            return Err(NativeAdapterError::new("test.type", "resource required", 0));
        };
        receiver.values.extend(value.values);
        Ok(())
    },
};

fn tracked() -> Tracked {
    Tracked {
        values: vec![1, 2],
        clones: Rc::new(Cell::new(0)),
    }
}

#[test]
fn reads_borrow_without_cloning_and_allocations_preserve_owner() {
    let value = tracked();
    let clones = value.clones.clone();
    let mut registry = ResourceRegistry::new();
    let handle = registry.insert_for_owner(7, value).unwrap();
    assert_eq!(
        ADAPTER
            .call(
                &mut registry,
                7,
                "read",
                &[NativeResourceValue::Resource(handle)]
            )
            .unwrap(),
        NativeResourceValue::Value(NativeValue::List(vec![1i64.into(), 2i64.into()]))
    );
    assert_eq!(clones.get(), 0);
    let NativeResourceValue::Resource(copy) = ADAPTER
        .call(
            &mut registry,
            7,
            "copy",
            &[NativeResourceValue::Resource(handle)],
        )
        .unwrap()
    else {
        panic!("expected resource");
    };
    assert_ne!(copy, handle);
    assert_eq!(clones.get(), 1, "only the explicit package copy clones");
    assert_eq!(registry.get_for_owner(copy, 7).unwrap().values, vec![1, 2]);
    assert_eq!(
        registry.get_for_owner(copy, 8).unwrap_err().code(),
        "resource.owner"
    );
    assert_eq!(registry.dispose_owner(7), 2);
    assert_eq!(
        registry.get_for_owner(copy, 7).unwrap_err().code(),
        "resource.stale_handle"
    );
}

#[test]
fn mutation_snapshots_self_arguments_once_without_copying_receiver() {
    let value = tracked();
    let clones = value.clones.clone();
    let mut registry = ResourceRegistry::new();
    let handle = registry.insert_for_owner(7, value).unwrap();
    let result = ADAPTER
        .call(
            &mut registry,
            7,
            "extend",
            &[
                NativeResourceValue::Resource(handle),
                NativeResourceValue::Resource(handle),
            ],
        )
        .unwrap();
    assert_eq!(result, NativeResourceValue::Resource(handle));
    assert_eq!(
        registry.get_for_owner(handle, 7).unwrap().values,
        vec![1, 2, 1, 2]
    );
    assert_eq!(clones.get(), 1);
    assert_eq!(registry.dispose_owner(7), 1);
}

#[test]
fn foreign_stale_and_forged_handles_fail_before_callbacks_or_clones() {
    let value = tracked();
    let clones = value.clones.clone();
    let mut registry = ResourceRegistry::new();
    let handle = registry.insert_for_owner(7, value).unwrap();
    let foreign = registry.insert_for_owner(8, tracked()).unwrap();
    let stale = registry.insert_for_owner(7, tracked()).unwrap();
    registry.dispose_for_owner(stale, 7).unwrap();
    let forged = NativeResourceHandle {
        generation: 2,
        ..handle
    };
    for (bad, code) in [
        (foreign, "resource.owner"),
        (stale, "resource.stale_handle"),
        (forged, "resource.stale_handle"),
    ] {
        for operation in ["read", "copy"] {
            assert_eq!(
                ADAPTER
                    .call(
                        &mut registry,
                        7,
                        operation,
                        &[NativeResourceValue::Resource(bad)]
                    )
                    .unwrap_err()
                    .code(),
                code
            );
        }
        for handles in [[bad, handle], [handle, bad]] {
            let args = handles.map(NativeResourceValue::Resource);
            assert_eq!(
                ADAPTER
                    .call(&mut registry, 7, "extend", &args)
                    .unwrap_err()
                    .code(),
                code
            );
        }
    }
    assert_eq!(clones.get(), 0);
    assert_eq!(
        registry.get_for_owner(handle, 7).unwrap().values,
        vec![1, 2]
    );
    assert_eq!(registry.dispose_owner(7), 1);
    assert_eq!(registry.dispose_owner(8), 1);
}

#[test]
fn contracts_types_and_package_errors_are_not_hidden() {
    let mut registry = ResourceRegistry::new();
    let handle = registry.insert_for_owner(7, tracked()).unwrap();
    for operation in ADAPTER.operations {
        let args = (0..operation.arity + 1)
            .map(|_| NativeResourceValue::Resource(handle))
            .collect::<Vec<_>>();
        assert_eq!(
            ADAPTER
                .call(&mut registry, 7, operation.operation, &args)
                .unwrap_err()
                .code(),
            "dispatch.arity"
        );
    }
    for name in ["", "read.extra", "Read", "read\0"] {
        assert_eq!(
            ADAPTER
                .call(&mut registry, 7, name, &[])
                .unwrap_err()
                .code(),
            "dispatch.unknown_operation"
        );
    }
    assert_eq!(
        ADAPTER
            .call(&mut registry, 7, "bad_contract", &[])
            .unwrap_err()
            .code(),
        "dispatch.type"
    );
    assert_eq!(
        ADAPTER
            .call(
                &mut registry,
                7,
                "extend",
                &[
                    NativeResourceValue::Value(NativeValue::Unit),
                    NativeResourceValue::Resource(handle)
                ]
            )
            .unwrap_err()
            .code(),
        "dispatch.type"
    );
    assert_eq!(
        ADAPTER
            .call(
                &mut registry,
                7,
                "extend",
                &[
                    NativeResourceValue::Resource(handle),
                    NativeResourceValue::Value(NativeValue::Unit)
                ]
            )
            .unwrap_err()
            .code(),
        "test.type"
    );
    assert_eq!(
        ADAPTER
            .call(
                &mut registry,
                7,
                "read",
                &[NativeResourceValue::Value(NativeValue::Unit)]
            )
            .unwrap_err()
            .code(),
        "test.type"
    );
    for (operation, offset) in [("fail", 17), ("fail_mutation", 19)] {
        let error = ADAPTER
            .call(
                &mut registry,
                7,
                operation,
                &[NativeResourceValue::Resource(handle)],
            )
            .unwrap_err();
        assert_eq!(error.code(), "test.failure");
        assert_eq!(error.offset(), offset);
    }
    assert_eq!(
        registry.get_for_owner(handle, 7).unwrap().values,
        vec![1, 2]
    );
    assert_eq!(registry.dispose_owner(7), 1);
}

#[test]
fn mismatched_returns_fail_before_insertion_or_mutation() {
    const CONTRACTS: &[NativeResourceOperation] = &[
        operation("unexpected_resource", 1, false, false),
        operation("unexpected_value", 1, false, true),
        operation("invalid_mutation", 1, true, false),
    ];
    let adapter = NativeResourceAdapter {
        operations: CONTRACTS,
        invoke: |operation, args: &[NativeResourceValue<&Tracked>]| match operation {
            "unexpected_resource" => {
                let [NativeResourceValue::Resource(value)] = args else {
                    panic!("expected borrowed resource");
                };
                Ok(NativeResourceValue::Resource((*value).clone()))
            }
            "unexpected_value" => Ok(NativeResourceValue::Value(NativeValue::Unit)),
            _ => panic!("invalid contract must never invoke a callback"),
        },
        mutate: |_, _, _| panic!("invalid contract must never mutate the receiver"),
    };
    let mut registry = ResourceRegistry::new();
    let handle = registry.insert_for_owner(7, tracked()).unwrap();
    for contract in CONTRACTS {
        let error = adapter
            .call(
                &mut registry,
                7,
                contract.operation,
                &[NativeResourceValue::Resource(handle)],
            )
            .unwrap_err();
        assert_eq!(error.code(), "dispatch.contract");
        assert!(error.message().contains(contract.operation));
        assert_eq!(
            registry.get_for_owner(handle, 7).unwrap().values,
            vec![1, 2]
        );
    }
    assert_eq!(
        registry.dispose_owner(7),
        1,
        "invalid output was not stored"
    );
}

#[test]
fn host_allocation_failure_is_reported_without_replacing_the_input() {
    struct Full(ResourceRegistry<Tracked>);
    impl NativeResourceStore<Tracked> for Full {
        fn borrow(
            &self,
            owner: u64,
            handle: NativeResourceHandle,
        ) -> Result<&Tracked, ResourceError> {
            self.0.get_for_owner(handle, owner)
        }
        fn borrow_mut(
            &mut self,
            owner: u64,
            handle: NativeResourceHandle,
        ) -> Result<&mut Tracked, ResourceError> {
            self.0.get_mut_for_owner(handle, owner)
        }
        fn insert(&mut self, _: u64, _: Tracked) -> Result<NativeResourceHandle, ResourceError> {
            Err(ResourceError::new("test.full", "registry is full"))
        }
    }
    let mut registry = ResourceRegistry::new();
    let handle = registry.insert_for_owner(7, tracked()).unwrap();
    let mut full = Full(registry);
    let error = ADAPTER
        .call(
            &mut full,
            7,
            "copy",
            &[NativeResourceValue::Resource(handle)],
        )
        .unwrap_err();
    assert_eq!(error.code(), "test.full");
    assert_eq!(error.message(), "registry is full");
    assert_eq!(full.0.dispose_owner(7), 1);
}
