//! Source operation contracts for the database package's actor-worker adapter.

pub fn operation_arity(operation: &str) -> Option<usize> {
    match operation {
        "std.db.postgres.connect"
        | "std.db.postgres.begin"
        | "std.db.postgres.commit"
        | "std.db.postgres.rollback" => Some(1),
        "std.db.postgres.string"
        | "std.db.postgres.int"
        | "std.db.postgres.bool"
        | "std.db.postgres.json" => Some(2),
        "std.db.postgres.query" | "std.db.postgres.query_one" | "std.db.postgres.execute" => {
            Some(3)
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "native_operations_test.rs"]
mod tests;
