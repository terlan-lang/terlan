use super::*;
use crate::FromNativeValue;

fn increment(name: &'static str) -> NativeContextBinding<i64> {
    NativeContextBinding::new(name, 1, |count: &mut i64, args| {
        *count += i64::from_native(&args[0])?;
        Ok((*count).into())
    })
}

fn granted(initial: i64) -> (NativeServices, Arc<Mutex<i64>>) {
    let context = Arc::new(Mutex::new(initial));
    let mut services = NativeServices::default();
    services
        .register_context(Arc::clone(&context), vec![increment("example.add")])
        .unwrap();
    (services, context)
}

#[test]
fn explicit_grants_preserve_shared_state_without_crossing_applications() {
    let (mut services, context) = granted(0);
    let previous_generation = services.clone();
    let (other, _) = granted(100);
    assert!(previous_generation.contains("example.add"));
    assert!(!previous_generation.contains("example.extra"));
    services
        .register_context(Arc::clone(&context), vec![increment("example.extra")])
        .unwrap();
    assert!(services.contains("example.extra"));
    assert!(!previous_generation.contains("example.extra"));
    assert_eq!(services.call("example.add", &[3.into()]), Ok(3.into()));
    assert_eq!(
        previous_generation.call("example.add", &[2.into()]),
        Ok(5.into())
    );
    assert_eq!(other.call("example.add", &[1.into()]), Ok(101.into()));
    assert_eq!(
        previous_generation
            .call("example.extra", &[9.into()])
            .unwrap_err()
            .code(),
        "native_service.unavailable"
    );
    assert_eq!(*context.lock().unwrap(), 5);
    assert_eq!(services.call("example.extra", &[4.into()]), Ok(9.into()));
    assert_eq!(
        format!("{previous_generation:?}"),
        "NativeServices { operations: [\"example.add\"] }"
    );
}

#[test]
fn malformed_arguments_and_unknown_names_do_not_touch_context() {
    let (services, context) = granted(7);
    assert_eq!(services.validate_arity("example.add", 1), Ok(()));
    for name in ["", "example.add.more", "example.ADD", "example.add "] {
        assert!(!services.contains(name));
        assert_eq!(
            services.validate_arity(name, 1).unwrap_err().code(),
            "native_service.unavailable"
        );
        assert_eq!(
            services.call(name, &[1.into()]).unwrap_err().code(),
            "native_service.unavailable"
        );
    }
    for args in [vec![], vec![1.into(), 2.into()]] {
        assert_eq!(
            services
                .validate_arity("example.add", args.len())
                .unwrap_err()
                .code(),
            "native_package.arguments"
        );
        assert_eq!(
            services.call("example.add", &args).unwrap_err().code(),
            "native_package.arguments"
        );
    }
    let error = services
        .call("example.add", &[NativeValue::String("1".into())])
        .unwrap_err();
    assert_eq!(error.code(), "dispatch.type");
    assert_eq!(*context.lock().unwrap(), 7);
}

#[test]
fn rejected_registration_batches_are_atomic_and_never_replace_a_grant() {
    let (mut services, original) = granted(0);
    let other = Arc::new(Mutex::new(100));
    for names in [["new", ""], ["new", "new"], ["new", "example.add"]] {
        let error = services
            .register_context(
                Arc::clone(&other),
                names.into_iter().map(increment).collect::<Vec<_>>(),
            )
            .unwrap_err();
        assert_eq!(error.code(), "native_service.registration");
        assert_eq!(
            services.call("new", &[1.into()]).unwrap_err().code(),
            "native_service.unavailable"
        );
    }
    services
        .register_context(Arc::clone(&other), Vec::new())
        .unwrap();
    assert_eq!(services.call("example.add", &[1.into()]), Ok(1.into()));
    assert_eq!(*original.lock().unwrap(), 1);
    assert_eq!(*other.lock().unwrap(), 100);
}

#[test]
fn context_lifetime_follows_grants_and_shards_serialize_mutation() {
    let (services, context) = granted(0);
    let weak = Arc::downgrade(&context);
    drop(context);
    let workers = (0..4)
        .map(|_| {
            let shard = services.clone();
            std::thread::spawn(move || {
                for _ in 0..64 {
                    shard.call("example.add", &[1.into()]).unwrap();
                }
            })
        })
        .collect::<Vec<_>>();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(services.call("example.add", &[0.into()]), Ok(256.into()));
    assert!(weak.upgrade().is_some());
    drop(services);
    assert!(weak.upgrade().is_none());
}

#[test]
fn poisoned_context_fails_closed_without_affecting_other_contexts() {
    let (mut services, context) = granted(0);
    let worker = std::thread::spawn(move || {
        let _guard = context.lock().unwrap();
        panic!("intentional poisoned test context");
    });
    assert!(worker.join().is_err());
    let error = services.call("example.add", &[1.into()]).unwrap_err();
    assert_eq!(error.code(), "native_service.poisoned");
    assert_eq!(error.domain(), ErrorDomain::NativeBoundary);
    assert_eq!(
        services.call("example.add", &[]).unwrap_err().code(),
        "native_package.arguments"
    );
    services
        .register_context(Arc::new(Mutex::new(10)), vec![increment("healthy")])
        .unwrap();
    assert_eq!(services.call("healthy", &[1.into()]), Ok(11.into()));
}
