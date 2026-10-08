use super::*;
use terlan_runtime_abi::{FromNativeValue, NativeValue};

#[test]
fn session_acquisition_preserves_exact_identity_and_expiration_through_package_bindings() {
    let service = VmHttpSessionService::new(VmHttpSessionRuntime::new("acquire", 100).unwrap());
    let services = service.native_services().unwrap();
    let acquire = |identity: &str| {
        let value = services
            .call("std.http.session.lookup", &[identity.into()])
            .unwrap();
        if let Some(identity) = Option::<String>::from_native(&value).unwrap() {
            return identity;
        }
        let value = services
            .call(
                "std.http.session.create",
                &[identity.into(), 100_i64.into()],
            )
            .unwrap();
        String::from_native(&value).unwrap()
    };
    let identity = acquire("");
    assert_eq!(identity.len(), 43);
    assert_eq!(acquire(&identity), identity);
    for requested in [
        format!(" {identity} "),
        format!("\u{2003}{identity}\u{2003}"),
        String::new(),
    ] {
        let next = acquire(&requested);
        assert_ne!(next, identity);
        assert_ne!(next, requested);
    }
    assert_eq!(
        service
            .with_storage(|state| state.snapshots().len())
            .unwrap(),
        4
    );
    let arguments = [identity.into()];
    for _ in 0..2 {
        assert_eq!(
            services.call("std.http.session.is_live", &arguments),
            Ok(true.into())
        );
    }
    assert_eq!(
        services.call("std.http.session.expire", &arguments),
        Ok(NativeValue::Unit)
    );
    assert_eq!(
        services.call("std.http.session.is_live", &arguments),
        Ok(false.into())
    );
}

#[test]
fn session_package_rejects_invalid_arguments_before_storage_mutation() {
    let mut state = VmHttpSessionRuntime::new("identity-boundary", 100).unwrap();
    let session = super::super::current(&mut state, None).unwrap().session;
    super::super::set(&mut state, &session, "key", "original").unwrap();
    let service = VmHttpSessionService::new(state);
    let services = service.native_services().unwrap();
    let invalid = [
        NativeValue::Int(0),
        NativeValue::Int(-1),
        NativeValue::Unit,
        None::<String>.into(),
        Some("").into(),
        Some("unknown").into(),
        NativeValue::Tuple(vec!["identity".into(), true.into()]),
    ];
    for (name, args) in [
        ("lookup", vec![session.managed_id().into()]),
        ("create", vec![session.managed_id().into(), 100_i64.into()]),
        ("get", vec![session.managed_id().into(), "key".into()]),
        (
            "set",
            vec![
                session.managed_id().into(),
                "key".into(),
                "replacement".into(),
            ],
        ),
        ("delete", vec![session.managed_id().into(), "key".into()]),
        ("rotate", vec![session.managed_id().into(), 100_i64.into()]),
        ("expire", vec![session.managed_id().into()]),
        ("is_live", vec![session.managed_id().into()]),
    ] {
        let name = format!("std.http.session.{name}");
        for count in 0..=4 {
            if count != args.len() {
                assert!(services
                    .call(&name, &vec!["identity".into(); count])
                    .is_err());
            }
        }
        for index in 0..args.len() {
            for value in &invalid {
                let mut arguments = args.clone();
                arguments[index] = value.clone();
                assert!(services.call(&name, &arguments).is_err());
            }
        }
        service
            .with_storage(|state| {
                assert_eq!(state.snapshots().len(), 1);
                assert!(VmHttpSessionRuntime::is_live(state, &session));
                assert_eq!(
                    super::super::get(state, &session, "key").unwrap(),
                    Some("original".into())
                );
            })
            .unwrap();
    }
}

#[test]
fn package_session_maintenance_releases_only_its_bounded_batch() {
    let mut state = VmHttpSessionRuntime::new("maintenance", 1).unwrap();
    for _ in 0..3 {
        super::super::current(&mut state, None).unwrap();
    }
    state.advance_ticks(1);
    let service = VmHttpSessionService::new(state);
    service.start_clock().unwrap();
    service.maintain(0).unwrap();
    service
        .with_storage(|state| assert_eq!(state.snapshots().len(), 3))
        .unwrap();
    service.maintain(2).unwrap();
    service
        .with_storage(|state| assert_eq!(state.snapshots().len(), 1))
        .unwrap();
    service.maintain(2).unwrap();
    service
        .with_storage(|state| {
            assert!(state.snapshots().is_empty());
            assert!(state.tables.snapshots().is_empty());
        })
        .unwrap();
}

#[test]
fn package_session_clock_and_cleanup_fail_closed_for_poisoned_contexts() {
    let service = VmHttpSessionService::new(VmHttpSessionRuntime::new("poison", 1).unwrap());
    let cloned = service.clone();
    assert!(
        std::thread::spawn(move || cloned.with_storage(|_| panic!("poison fixture")))
            .join()
            .is_err()
    );
    assert!(service
        .start_clock()
        .unwrap_err()
        .to_string()
        .contains("lock poisoned"));
    assert!(service
        .maintain(1)
        .unwrap_err()
        .to_string()
        .contains("lock poisoned"));
}
