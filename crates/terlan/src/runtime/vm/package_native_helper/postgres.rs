//! Closed source projection for database resources owned by external workers.
use super::{
    direct_std::{native_handle, native_handle_value},
    result_error, result_ok, ReplValue, VmRuntimeResult,
};
use crate::runtime::vm::pure_native::PureNativeCapabilityRequest;
use crate::terlan_native::{json, postgres};
use crate::terlan_native_boundary::{
    handle::NativeBoundaryHandle as Handle,
    resource::{ResourceError, ResourceRegistry, ResourceStore, ResourceValue},
    term::{NativeBoundaryReplyTerm as Reply, NativeBoundaryTerm as Term},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Kind {
    Pool,
    Connection,
    Row,
}
impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Pool => "std.db.Postgres.Pool",
            Self::Connection => "std.db.Postgres.Connection",
            Self::Row => "std.db.Postgres.Row",
        }
    }
}
struct Resource {
    remote: Handle,
    kind: Kind,
}
#[derive(Default)]
pub(super) struct Adapter {
    resources: ResourceRegistry<Resource>,
}
#[derive(Clone, Copy)]
pub(super) enum Projection {
    Handle(Kind),
    Rows,
    OptionalRow,
    Int,
    Bool,
    String,
    Json,
    Unit,
}
pub(super) struct Request {
    pub(super) operation: String,
    pub(super) arguments: Vec<Term>,
    pub(super) projection: Projection,
}
impl Adapter {
    pub(super) fn prepare(
        &mut self,
        owner: u64,
        request: &PureNativeCapabilityRequest,
        json_store: &ResourceStore,
    ) -> VmRuntimeResult<Option<Request>> {
        let Some(operation) = request.operation.strip_prefix("std.db.postgres.") else {
            return Ok(None);
        };
        let args = request
            .package_arguments
            .as_deref()
            .ok_or("error[postgres.arguments]: database operation requires source arguments")?;
        if crate::std_native_packages::postgres::operation_arity(&request.operation)
            != Some(args.len())
        {
            return Err("error[postgres.arguments]: unsupported database call shape".into());
        }
        let (arguments, projection) = match (operation, args) {
            ("connect", [ReplValue::Record { fields, .. }]) => {
                let url = fields
                    .iter()
                    .find_map(|(key, value)| (key == "url").then_some(value))
                    .ok_or("error[postgres.arguments]: configuration requires URL")?;
                (vec![text(url)?], Projection::Handle(Kind::Pool))
            }
            ("query" | "query_one" | "execute", [target, sql, ReplValue::List(params)]) => {
                let target = self.remote(owner, target, &[Kind::Pool, Kind::Connection])?;
                let params = params
                    .iter()
                    .map(|value| {
                        let handle = source_handle(owner, value, "std.data.Json.Json")?;
                        json_store
                            .validate_owner(handle, owner)
                            .map_err(resource_error)?;
                        let value = json_store.json(handle).map_err(resource_error)?;
                        json::stringify(value).map(Term::Text).map_err(|_| {
                            "error[postgres.arguments]: cannot encode JSON parameter".into()
                        })
                    })
                    .collect::<VmRuntimeResult<Vec<_>>>()?;
                let projection = match operation {
                    "execute" => Projection::Int,
                    "query_one" => Projection::OptionalRow,
                    _ => Projection::Rows,
                };
                (vec![target, text(sql)?, Term::List(params)], projection)
            }
            ("begin", [pool]) => (
                vec![self.remote(owner, pool, &[Kind::Pool])?],
                Projection::Handle(Kind::Connection),
            ),
            ("commit" | "rollback", [connection]) => {
                let remote = self.remote(owner, connection, &[Kind::Connection])?;
                let handle = source_handle(owner, connection, Kind::Connection.name())?;
                self.resources
                    .dispose_for_owner(handle, owner)
                    .map_err(resource_error)?;
                (vec![remote], Projection::Unit)
            }
            ("string" | "int" | "bool" | "json", [row, column]) => {
                let projection = match operation {
                    "string" => Projection::String,
                    "int" => Projection::Int,
                    "bool" => Projection::Bool,
                    _ => Projection::Json,
                };
                (
                    vec![self.remote(owner, row, &[Kind::Row])?, text(column)?],
                    projection,
                )
            }
            _ => return Err("error[postgres.arguments]: unsupported database call shape".into()),
        };
        Ok(Some(Request {
            operation: format!("runtime.postgres.{operation}"),
            arguments,
            projection,
        }))
    }
    pub(super) fn complete(
        &mut self,
        owner: u64,
        projection: Projection,
        reply: Reply,
        json_store: &mut ResourceStore,
    ) -> VmRuntimeResult<ReplValue> {
        let term = match reply {
            Reply::Ok(term) => term,
            Reply::Error { code, message, .. } => {
                let code = if postgres::SOURCE_ERROR_CODES.contains(&code.as_str()) {
                    code
                } else {
                    "postgres.operation".into()
                };
                return Ok(result_error(code, message));
            }
        };
        let value = match (projection, term) {
            (Projection::Handle(kind), term @ Term::Handle { .. }) => {
                self.insert(owner, kind, term)?
            }
            (Projection::Rows, Term::List(rows)) => ReplValue::List(
                rows.into_iter()
                    .map(|row| self.insert(owner, Kind::Row, row))
                    .collect::<VmRuntimeResult<_>>()?,
            ),
            (Projection::OptionalRow, Term::List(mut rows)) if rows.len() <= 1 => {
                match rows.pop() {
                    None => ReplValue::Record {
                        name: "None".into(),
                        fields: vec![],
                    },
                    Some(row) => ReplValue::Record {
                        name: "Some".into(),
                        fields: vec![("value".into(), self.insert(owner, Kind::Row, row)?)],
                    },
                }
            }
            (Projection::Int, Term::Int(value)) => ReplValue::Int(value),
            (Projection::Bool, Term::Bool(value)) => ReplValue::Bool(value),
            (Projection::String, Term::Text(value)) => ReplValue::String(value),
            (Projection::Unit, Term::Unit) => ReplValue::Unit,
            (Projection::Json, Term::Text(value)) => {
                let value = json::parse(&value)
                    .map_err(|_| "error[postgres.protocol]: worker returned malformed JSON")?;
                let handle = json_store
                    .insert_for_owner(owner, ResourceValue::Json(value))
                    .map_err(resource_error)?;
                native_handle_value(owner, handle, "std.data.Json.Json")?
            }
            _ => {
                return Err(
                    "error[postgres.protocol]: worker returned an incompatible value".into(),
                )
            }
        };
        Ok(result_ok(value))
    }
    fn remote(&self, owner: u64, value: &ReplValue, kinds: &[Kind]) -> VmRuntimeResult<Term> {
        let ReplValue::Record { fields, .. } = value else {
            return Err("error[postgres.resource]: expected database handle".into());
        };
        let (handle, type_name, _) =
            native_handle(fields).ok_or("error[postgres.resource]: expected database handle")??;
        let resource = self
            .resources
            .get_for_owner(handle, owner)
            .map_err(resource_error)?;
        if !kinds.contains(&resource.kind) || type_name != resource.kind.name() {
            return Err("error[postgres.resource]: database resource kind mismatch".into());
        }
        source_handle(owner, value, resource.kind.name())?;
        Ok(Term::Handle {
            id: resource.remote.id,
            generation: resource.remote.generation,
        })
    }
    fn insert(&mut self, owner: u64, kind: Kind, term: Term) -> VmRuntimeResult<ReplValue> {
        let Term::Handle { id, generation } = term else {
            return Err("error[postgres.protocol]: worker returned a non-handle resource".into());
        };
        let handle = self
            .resources
            .insert_for_owner(
                owner,
                Resource {
                    remote: Handle { id, generation },
                    kind,
                },
            )
            .map_err(resource_error)?;
        native_handle_value(owner, handle, kind.name())
    }
    pub(super) fn close_owner(&mut self, owner: u64) {
        self.resources.dispose_owner(owner);
    }
}
fn source_handle(owner: u64, value: &ReplValue, expected_type: &str) -> VmRuntimeResult<Handle> {
    let ReplValue::Record { fields, .. } = value else {
        return Err("error[postgres.resource]: expected resource handle".into());
    };
    let (handle, type_name, claimed_owner) =
        native_handle(fields).ok_or("error[postgres.resource]: expected resource handle")??;
    if claimed_owner != owner.to_string() || type_name != expected_type {
        return Err("error[postgres.resource]: resource owner or kind mismatch".into());
    }
    Ok(handle)
}
fn text(value: &ReplValue) -> VmRuntimeResult<Term> {
    match value {
        ReplValue::String(value) => Ok(Term::Text(value.clone())),
        ReplValue::StringBytes(value) => std::str::from_utf8(value)
            .map(|value| Term::Text(value.into()))
            .map_err(|_| "error[postgres.arguments]: expected UTF-8 string".into()),
        _ => Err("error[postgres.arguments]: expected string".into()),
    }
}
fn resource_error(error: ResourceError) -> super::VmRuntimeError {
    format!("error[{}]: {}", error.code(), error.message()).into()
}

#[cfg(test)]
#[path = "postgres_test.rs"]
mod tests;
