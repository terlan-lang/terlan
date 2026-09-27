//! Worker-local pools, transactions and rows backed by the existing libpq client.
use super::{error, shape_error, CapabilityValue as Value, Result};
use crate::runtime::vm::{
    postgres::{VmPostgresDecodedValue, VmPostgresRow, VmPostgresTransaction},
    postgres_command::VmPostgresCommandClient,
};
use crate::terlan_native::{json, postgres};
use crate::terlan_native_boundary::{
    handle::NativeBoundaryHandle as Handle,
    resource::{ResourceError, ResourceRegistry},
};

enum Resource {
    Pool(Box<VmPostgresCommandClient>),
    Transaction(Handle, VmPostgresTransaction),
    Row(Handle, VmPostgresRow),
}

#[derive(Default)]
pub(super) struct Driver {
    resources: ResourceRegistry<Resource>,
    allocated: usize,
}

impl Driver {
    pub(super) fn execute(&mut self, owner: u64, operation: &str, args: &[Value]) -> Result<Value> {
        match (operation, args) {
            ("runtime.postgres.connect", [Value::Text(url)]) => {
                self.reserve(1)?;
                let client = VmPostgresCommandClient::connect(&postgres::Config::new(url))
                    .map_err(client_error)?;
                self.insert(owner, Resource::Pool(Box::new(client)))
            }
            (
                "runtime.postgres.query"
                | "runtime.postgres.query_one"
                | "runtime.postgres.execute",
                [target, Value::Text(sql), Value::List(params)],
            ) => {
                let target = handle(target)?;
                let params = params
                    .iter()
                    .map(|value| match value {
                        Value::Text(value) => json::parse(value).map_err(|_| shape_error()),
                        _ => Err(shape_error()),
                    })
                    .collect::<Result<Vec<_>>>()?;
                let (pool, transaction) = self.target(owner, target)?;
                let client = self.pool(owner, pool)?;
                if operation == "runtime.postgres.execute" {
                    let count = match transaction {
                        Some(transaction) => client.execute_transaction(transaction, sql, params),
                        None => client.execute(sql, params),
                    }
                    .map_err(client_error)?;
                    return Ok(Value::Int(count));
                }
                let rows = if operation == "runtime.postgres.query_one" {
                    match transaction {
                        Some(transaction) => client.query_one_transaction(transaction, sql, params),
                        None => client.query_one(sql, params),
                    }
                    .map_err(client_error)?
                    .into_iter()
                    .collect()
                } else {
                    match transaction {
                        Some(transaction) => client.query_transaction(transaction, sql, params),
                        None => client.query(sql, params),
                    }
                    .map_err(client_error)?
                };
                if let Err(error) = self.reserve(rows.len()) {
                    // The command client retains decoded rows. Revoke the owner on
                    // exhaustion so rejected queries cannot grow hidden row storage.
                    self.resources.dispose_owner(owner);
                    return Err(error);
                }
                Ok(Value::List(
                    rows.into_iter()
                        .map(|row| self.insert(owner, Resource::Row(pool, row)))
                        .collect::<Result<Vec<_>>>()?,
                ))
            }
            ("runtime.postgres.begin", [pool]) => {
                let pool = handle(pool)?;
                self.reserve(1)?;
                let transaction = self.pool(owner, pool)?.begin().map_err(client_error)?;
                self.insert(owner, Resource::Transaction(pool, transaction))
            }
            ("runtime.postgres.commit" | "runtime.postgres.rollback", [transaction]) => {
                let handle = handle(transaction)?;
                let (pool, transaction) = match self
                    .resources
                    .get_for_owner(handle, owner)
                    .map_err(resource_error)?
                {
                    Resource::Transaction(pool, transaction) => (*pool, *transaction),
                    _ => return Err(shape_error()),
                };
                // A terminal attempt revokes the source connection even if acknowledgement is lost.
                self.resources
                    .dispose_for_owner(handle, owner)
                    .map_err(resource_error)?;
                self.pool(owner, pool)?
                    .finish_transaction(transaction, operation == "runtime.postgres.commit")
                    .map_err(client_error)?;
                Ok(Value::Unit)
            }
            (
                "runtime.postgres.string"
                | "runtime.postgres.int"
                | "runtime.postgres.bool"
                | "runtime.postgres.json",
                [row, Value::Text(column)],
            ) => {
                let (pool, row) = match self
                    .resources
                    .get_for_owner(handle(row)?, owner)
                    .map_err(resource_error)?
                {
                    Resource::Row(pool, row) => (*pool, *row),
                    _ => return Err(shape_error()),
                };
                let value = self
                    .pool(owner, pool)?
                    .decode_dynamic(row, column)
                    .map_err(client_error)?;
                match (operation, value) {
                    ("runtime.postgres.string", VmPostgresDecodedValue::String(value)) => {
                        Ok(Value::Text(value))
                    }
                    ("runtime.postgres.int", VmPostgresDecodedValue::Int(value)) => {
                        Ok(Value::Int(value))
                    }
                    ("runtime.postgres.bool", VmPostgresDecodedValue::Bool(value)) => {
                        Ok(Value::Bool(value))
                    }
                    ("runtime.postgres.json", VmPostgresDecodedValue::Json(value)) => {
                        Ok(Value::Text(value))
                    }
                    _ => Err(error(
                        "postgres.column_type",
                        "column does not have the requested non-null type",
                    )),
                }
            }
            _ => Err(shape_error()),
        }
    }

    fn reserve(&self, count: usize) -> Result<()> {
        if count > 4096 - self.allocated {
            return Err(error(
                "postgres.resource_limit",
                "database resource limit reached",
            ));
        }
        Ok(())
    }
    fn insert(&mut self, owner: u64, value: Resource) -> Result<Value> {
        self.reserve(1)?;
        let handle = self
            .resources
            .insert_for_owner(owner, value)
            .map_err(resource_error)?;
        self.allocated += 1;
        Ok(Value::Handle(handle.into()))
    }
    fn pool(&mut self, owner: u64, handle: Handle) -> Result<&mut VmPostgresCommandClient> {
        match self
            .resources
            .get_mut_for_owner(handle, owner)
            .map_err(resource_error)?
        {
            Resource::Pool(client) => Ok(client),
            _ => Err(shape_error()),
        }
    }
    fn target(
        &self,
        owner: u64,
        handle: Handle,
    ) -> Result<(Handle, Option<VmPostgresTransaction>)> {
        match self
            .resources
            .get_for_owner(handle, owner)
            .map_err(resource_error)?
        {
            Resource::Pool(_) => Ok((handle, None)),
            Resource::Transaction(pool, transaction) => Ok((*pool, Some(*transaction))),
            _ => Err(shape_error()),
        }
    }
}
fn handle(value: &Value) -> Result<Handle> {
    match value {
        Value::Handle(handle) => Ok((*handle).into()),
        _ => Err(shape_error()),
    }
}
fn resource_error(
    value: ResourceError,
) -> crate::terlan_native_boundary::term::NativeBoundaryReplyTerm {
    error(value.code(), value.message())
}
fn client_error(value: String) -> crate::terlan_native_boundary::term::NativeBoundaryReplyTerm {
    match value
        .strip_prefix("error[")
        .and_then(|value| value.split_once("]:"))
    {
        Some(("postgres.timeout" | "postgres.timed_out", _)) => error(
            "postgres.indeterminate",
            "database deadline expired; a mutation may have occurred; do not retry automatically",
        ),
        Some((code, message)) => error(code, message.trim()),
        None => error("postgres.operation", "database operation failed"),
    }
}
