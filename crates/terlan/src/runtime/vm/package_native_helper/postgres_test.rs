use super::*;
fn remote(id: u64) -> Term {
    Term::Handle { id, generation: 1 }
}
fn request(operation: &str, args: Vec<ReplValue>) -> PureNativeCapabilityRequest {
    PureNativeCapabilityRequest {
        capability: "package-native".into(),
        operation: format!("std.db.postgres.{operation}"),
        arguments: vec![],
        package_arguments: Some(args),
        result_type: crate::runtime::native_image::TvmBoundaryType::Unit,
    }
}
#[test]
fn database_handles_reject_foreign_owner_wrong_kind_stale_and_closed_actor() {
    let mut adapter = Adapter::default();
    let pool = adapter.insert(7, Kind::Pool, remote(1)).unwrap();
    assert!(adapter.remote(8, &pool, &[Kind::Pool]).is_err());
    assert!(adapter.remote(7, &pool, &[Kind::Row]).is_err());
    let mut forged = pool.clone();
    if let ReplValue::Record { fields, .. } = &mut forged {
        fields
            .iter_mut()
            .find(|(field, _)| field == "$native_generation")
            .unwrap()
            .1 = ReplValue::Int(2);
    }
    assert!(adapter.remote(7, &forged, &[Kind::Pool]).is_err());
    adapter.close_owner(7);
    let fresh = adapter.insert(7, Kind::Pool, remote(1)).unwrap();
    assert_ne!(pool, fresh);
    assert!(adapter.remote(7, &pool, &[Kind::Pool]).is_err());
    assert_eq!(adapter.remote(7, &fresh, &[Kind::Pool]).unwrap(), remote(1));
}
#[test]
fn transaction_handle_is_revoked_before_terminal_submission() {
    let mut adapter = Adapter::default();
    let connection = adapter.insert(7, Kind::Connection, remote(2)).unwrap();
    let store = ResourceStore::default();
    let pending = adapter
        .prepare(7, &request("commit", vec![connection.clone()]), &store)
        .unwrap()
        .unwrap();
    assert_eq!(pending.operation, "runtime.postgres.commit");
    assert!(adapter
        .prepare(7, &request("rollback", vec![connection]), &store)
        .is_err());
}
#[test]
fn json_crosses_process_boundary_as_text_and_retains_owner_on_return() {
    let mut adapter = Adapter::default();
    let pool = adapter.insert(7, Kind::Pool, remote(1)).unwrap();
    let mut store = ResourceStore::default();
    let value = json::parse("{\"answer\":42}").unwrap();
    let handle = store
        .insert_for_owner(7, ResourceValue::Json(value.clone()))
        .unwrap();
    let param = native_handle_value(7, handle, "std.data.Json.Json").unwrap();
    let pending = adapter
        .prepare(
            7,
            &request(
                "query",
                vec![
                    pool,
                    ReplValue::String("SELECT $1::jsonb".into()),
                    ReplValue::List(vec![param]),
                ],
            ),
            &store,
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        pending.arguments[2],
        Term::List(vec![Term::Text("{\"answer\":42}".into())])
    );
    let result = adapter
        .complete(
            7,
            Projection::Json,
            Reply::Ok(Term::Text("{\"answer\":42}".into())),
            &mut store,
        )
        .unwrap();
    let ReplValue::Record { fields, .. } = result else {
        panic!("Ok record");
    };
    let handle = source_handle(7, &fields[0].1, "std.data.Json.Json").unwrap();
    assert_eq!(store.json(handle).unwrap(), &value);
    assert!(store.validate_owner(handle, 8).is_err());
}
#[test]
fn projections_reject_malformed_rows_and_use_closed_error_atoms() {
    let mut adapter = Adapter::default();
    let mut store = ResourceStore::default();
    assert!(adapter
        .complete(
            7,
            Projection::OptionalRow,
            Reply::Ok(Term::List(vec![remote(1), remote(2)])),
            &mut store
        )
        .is_err());
    let none = adapter
        .complete(
            7,
            Projection::OptionalRow,
            Reply::Ok(Term::List(vec![])),
            &mut store,
        )
        .unwrap();
    assert_eq!(
        none,
        result_ok(ReplValue::Record {
            name: "None".into(),
            fields: vec![]
        })
    );
    let error = adapter
        .complete(
            7,
            Projection::Int,
            Reply::Error {
                code: "untrusted.dynamic.atom".into(),
                message: "diagnostic".into(),
                offset: 0,
            },
            &mut store,
        )
        .unwrap();
    assert_eq!(
        error,
        result_error("postgres.operation".into(), "diagnostic".into())
    );
}

#[test]
fn terminal_transaction_reply_preserves_unit_representation() {
    let mut adapter = Adapter::default();
    assert_eq!(
        adapter
            .complete(
                7,
                Projection::Unit,
                Reply::Ok(Term::Unit),
                &mut ResourceStore::default()
            )
            .unwrap(),
        result_ok(ReplValue::Unit)
    );
}
