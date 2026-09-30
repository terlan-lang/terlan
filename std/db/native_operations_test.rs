use super::operation_arity;

#[test]
fn database_contract_preserves_arities_and_rejects_unknown_names() {
    for (operation, arity) in [
        ("connect", 1),
        ("begin", 1),
        ("commit", 1),
        ("rollback", 1),
        ("string", 2),
        ("int", 2),
        ("bool", 2),
        ("json", 2),
        ("query", 3),
        ("query_one", 3),
        ("execute", 3),
    ] {
        assert_eq!(
            operation_arity(&format!("std.db.postgres.{operation}")),
            Some(arity)
        );
        assert_eq!(
            operation_arity(&format!("app.db.postgres.{operation}")),
            None
        );
    }
    for operation in [
        "",
        "std.db.postgres.transaction",
        "std.db.postgres.commit.extra",
    ] {
        assert_eq!(operation_arity(operation), None);
    }
}
