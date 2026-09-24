//! Conversion between resource handles and pure adapter values.

use super::*;

/// Decodes bridge-facing arguments into pure dispatch values.
///
/// Inputs:
/// - `store`: resource store used to resolve opaque handles.
/// - `operation`: compiler-native operation id.
/// - `args`: bridge-facing operation arguments.
///
/// Output:
/// - Pure dispatch values suitable for `dispatch`.
/// - `Err(DispatchError)` when a handle is stale or has the wrong kind.
///
/// Transformation:
/// - Resolves handles according to the operation family and clones the
///   adapter-owned value for pure dispatch.
pub(super) fn decode_bridge_args(
    store: &ResourceStore,
    operation: &str,
    args: &[NativeBoundaryBridgeValue],
) -> Result<Vec<NativeBoundaryValue>, DispatchError> {
    args.iter()
        .enumerate()
        .map(|(index, arg)| decode_bridge_arg(store, operation, index, arg))
        .collect()
}

/// Decodes one bridge-facing argument into a pure dispatch value.
///
/// Inputs:
/// - `store`: resource store used to resolve opaque handles.
/// - `operation`: compiler-native operation id.
/// - `index`: argument index for diagnostics.
/// - `arg`: bridge-facing argument.
///
/// Output:
/// - Pure dispatch value.
/// - `Err(DispatchError)` for unsupported bridge value shapes.
///
/// Transformation:
/// - Converts primitive bridge values directly and resolves handles to the
///   resource kind implied by the operation namespace.
fn decode_bridge_arg(
    store: &ResourceStore,
    operation: &str,
    index: usize,
    arg: &NativeBoundaryBridgeValue,
) -> Result<NativeBoundaryValue, DispatchError> {
    match arg {
        NativeBoundaryBridgeValue::Tuple(values) => values
            .iter()
            .map(|value| decode_bridge_arg(store, operation, index, value))
            .collect::<Result<Vec<_>, _>>()
            .map(NativeBoundaryValue::Tuple),
        NativeBoundaryBridgeValue::Unit => Ok(NativeBoundaryValue::Unit),
        NativeBoundaryBridgeValue::Text(value) => Ok(NativeBoundaryValue::Text(value.clone())),
        NativeBoundaryBridgeValue::Bytes(value) => Ok(NativeBoundaryValue::Bytes(value.clone())),
        NativeBoundaryBridgeValue::Int(value) => Ok(NativeBoundaryValue::Int(*value)),
        NativeBoundaryBridgeValue::Float(value) => Ok(NativeBoundaryValue::Float(*value)),
        NativeBoundaryBridgeValue::Bool(value) => Ok(NativeBoundaryValue::Bool(*value)),
        NativeBoundaryBridgeValue::PostgresConfig(value) => {
            Ok(NativeBoundaryValue::PostgresConfig(value.clone()))
        }
        NativeBoundaryBridgeValue::Handle(handle) if operation == "std.http.response.json" => store
            .json(*handle)
            .cloned()
            .map(NativeBoundaryValue::Json)
            .map_err(dispatch_resource_error),
        NativeBoundaryBridgeValue::Handle(handle) if operation.starts_with("std.data.json.") => {
            store
                .json(*handle)
                .cloned()
                .map(NativeBoundaryValue::Json)
                .map_err(dispatch_resource_error)
        }
        NativeBoundaryBridgeValue::Handle(handle) if operation.starts_with("std.regex.regex.") => {
            store
                .regex(*handle)
                .cloned()
                .map(NativeBoundaryValue::Regex)
                .map_err(dispatch_resource_error)
        }
        NativeBoundaryBridgeValue::Handle(handle) if operation.starts_with("std.http.request.") => {
            store
                .http_request(*handle)
                .cloned()
                .map(NativeBoundaryValue::HttpRequest)
                .map_err(dispatch_resource_error)
        }
        NativeBoundaryBridgeValue::Handle(handle) if operation.starts_with("std.http.cookies.") => {
            store
                .http_cookie_jar(*handle)
                .cloned()
                .map(NativeBoundaryValue::HttpCookieJar)
                .map_err(dispatch_resource_error)
        }
        NativeBoundaryBridgeValue::Handle(handle)
            if operation.starts_with("std.http.response.") =>
        {
            store
                .http_response(*handle)
                .cloned()
                .map(NativeBoundaryValue::HttpResponse)
                .map_err(dispatch_resource_error)
        }
        NativeBoundaryBridgeValue::Handle(handle) if operation.starts_with("std.io.path.") => store
            .path(*handle)
            .cloned()
            .map(NativeBoundaryValue::Path)
            .map_err(dispatch_resource_error),
        NativeBoundaryBridgeValue::Handle(handle) if operation.starts_with("std.net.uri.") => store
            .uri(*handle)
            .cloned()
            .map(NativeBoundaryValue::Uri)
            .map_err(dispatch_resource_error),
        NativeBoundaryBridgeValue::Handle(handle)
            if matches!(
                operation,
                "std.db.postgres.query"
                    | "std.db.postgres.query_one"
                    | "std.db.postgres.execute"
                    | "std.db.postgres.transaction"
            ) && index == 0 =>
        {
            store
                .postgres_pool(*handle)
                .cloned()
                .map(NativeBoundaryValue::PostgresPool)
                .map_err(dispatch_resource_error)
        }
        NativeBoundaryBridgeValue::Handle(handle)
            if matches!(
                operation,
                "std.db.postgres.string"
                    | "std.db.postgres.int"
                    | "std.db.postgres.bool"
                    | "std.db.postgres.json"
            ) && index == 0 =>
        {
            store
                .postgres_row(*handle)
                .cloned()
                .map(NativeBoundaryValue::PostgresRow)
                .map_err(dispatch_resource_error)
        }
        NativeBoundaryBridgeValue::Handle(_) => {
            Err(type_error(operation, index, "non-handle value"))
        }
        NativeBoundaryBridgeValue::OptionalText(_)
        | NativeBoundaryBridgeValue::OptionalHandle(_) => {
            Err(type_error(operation, index, "non-optional argument"))
        }
        NativeBoundaryBridgeValue::Atom(value) => Ok(NativeBoundaryValue::Atom(value.clone())),
        NativeBoundaryBridgeValue::Record { name, fields } => Ok(NativeBoundaryValue::Record {
            name: name.clone(),
            fields: fields
                .iter()
                .enumerate()
                .map(|(field_index, (name, value))| {
                    decode_bridge_arg(store, operation, field_index, value)
                        .map(|value| (name.clone(), value))
                })
                .collect::<Result<Vec<_>, _>>()?,
        }),
        NativeBoundaryBridgeValue::List(values)
            if ((matches!(
                operation,
                "std.data.json.string_field_rows"
                    | "std.data.json.string_fields"
                    | "std.data.json.required_fields"
                    | "std.data.json.required_field_rows"
            ) && index == 1)
                || (operation == "std.data.json.required_fields" && matches!(index, 2 | 3))
                || (operation == "std.data.json.required_field_rows"
                    && matches!(index, 2 | 3))
                || (operation == "std.data.json.required_field_rows_page"
                    && matches!(index, 3..=5))
                || (operation == "std.data.json.nested_string_field_rows"
                    && matches!(index, 1 | 3))
                || (operation == "std.data.json.nested_string_field_rows_page"
                    && matches!(index, 3 | 5))
                || (operation == "std.data.json.string_object_rows" && index == 0))
                || (operation == "std.crypto.hash.sha256_framed" && index == 0)
                || (operation == "std.crypto.hash.sha256_nul_separated" && index == 0)
                || (operation == "std.crypto.hash.sha256_domain_framed" && index == 1) =>
        {
            Ok(NativeBoundaryValue::List(
                values
                    .iter()
                    .enumerate()
                    .map(|(list_index, value)| match value {
                        NativeBoundaryBridgeValue::Text(value) => {
                            Ok(NativeBoundaryValue::Text(value.clone()))
                        }
                        _ => Err(type_error(operation, list_index, "String")),
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ))
        }
        NativeBoundaryBridgeValue::List(rows)
            if operation == "std.data.json.string_object_rows" && index == 1 =>
        {
            Ok(NativeBoundaryValue::List(
                rows.iter()
                    .enumerate()
                    .map(|(row_index, row)| {
                        let NativeBoundaryBridgeValue::List(values) = row else {
                            return Err(type_error(operation, row_index, "List[String]"));
                        };
                        values
                            .iter()
                            .enumerate()
                            .map(|(value_index, value)| match value {
                                NativeBoundaryBridgeValue::Text(value) => {
                                    Ok(NativeBoundaryValue::Text(value.clone()))
                                }
                                _ => Err(type_error(operation, value_index, "String")),
                            })
                            .collect::<Result<Vec<_>, _>>()
                            .map(NativeBoundaryValue::List)
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ))
        }
        NativeBoundaryBridgeValue::List(values) => Ok(NativeBoundaryValue::JsonList(
            values
                .iter()
                .enumerate()
                .map(|(list_index, value)| match value {
                    NativeBoundaryBridgeValue::Handle(handle) => store
                        .json(*handle)
                        .cloned()
                        .map_err(dispatch_resource_error),
                    _ => Err(type_error(operation, list_index, "Json handle")),
                })
                .collect::<Result<Vec<_>, _>>()?,
        )),
    }
}

/// Encodes a pure dispatch result into a bridge-facing value.
///
/// Inputs:
/// - `store`: resource store that will own opaque adapter outputs.
/// - `value`: pure dispatch result.
///
/// Output:
/// - Bridge-facing result with opaque values represented as handles.
/// - `Err(DispatchError)` when resource insertion fails.
///
/// Transformation:
/// - Stores JSON/path/URI outputs in the resource store and returns only their
///   handles across the bridge surface.
pub(super) fn encode_bridge_result(
    store: &mut ResourceStore,
    caller_process_id: u64,
    value: NativeBoundaryValue,
) -> Result<NativeBoundaryBridgeValue, DispatchError> {
    match value {
        NativeBoundaryValue::Tuple(values) => values
            .into_iter()
            .map(|value| encode_bridge_result(store, caller_process_id, value))
            .collect::<Result<Vec<_>, _>>()
            .map(NativeBoundaryBridgeValue::Tuple),
        NativeBoundaryValue::Unit => Ok(NativeBoundaryBridgeValue::Unit),
        NativeBoundaryValue::Text(value) => Ok(NativeBoundaryBridgeValue::Text(value)),
        NativeBoundaryValue::Bytes(value) => Ok(NativeBoundaryBridgeValue::Bytes(value)),
        NativeBoundaryValue::Int(value) => Ok(NativeBoundaryBridgeValue::Int(value)),
        NativeBoundaryValue::Float(value) => Ok(NativeBoundaryBridgeValue::Float(value)),
        NativeBoundaryValue::Bool(value) => Ok(NativeBoundaryBridgeValue::Bool(value)),
        NativeBoundaryValue::Atom(value) => Ok(NativeBoundaryBridgeValue::Atom(value)),
        NativeBoundaryValue::Record { name, fields } => Ok(NativeBoundaryBridgeValue::Record {
            name,
            fields: fields
                .into_iter()
                .map(|(name, value)| {
                    encode_bridge_result(store, caller_process_id, value).map(|value| (name, value))
                })
                .collect::<Result<Vec<_>, _>>()?,
        }),
        NativeBoundaryValue::List(values) => Ok(NativeBoundaryBridgeValue::List(
            values
                .into_iter()
                .map(|value| encode_bridge_result(store, caller_process_id, value))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        NativeBoundaryValue::Json(value) => store
            .insert_for_owner(caller_process_id, ResourceValue::Json(value))
            .map(NativeBoundaryBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        NativeBoundaryValue::Regex(value) => store
            .insert_for_owner(caller_process_id, ResourceValue::Regex(value))
            .map(NativeBoundaryBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        NativeBoundaryValue::HttpRequest(value) => store
            .insert_for_owner(caller_process_id, ResourceValue::HttpRequest(value))
            .map(NativeBoundaryBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        NativeBoundaryValue::HttpResponse(value) => store
            .insert_for_owner(caller_process_id, ResourceValue::HttpResponse(value))
            .map(NativeBoundaryBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        NativeBoundaryValue::HttpCookieJar(value) => store
            .insert_for_owner(caller_process_id, ResourceValue::HttpCookieJar(value))
            .map(NativeBoundaryBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        NativeBoundaryValue::Path(value) => store
            .insert_for_owner(caller_process_id, ResourceValue::Path(value))
            .map(NativeBoundaryBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        NativeBoundaryValue::Uri(value) => store
            .insert_for_owner(caller_process_id, ResourceValue::Uri(value))
            .map(NativeBoundaryBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        NativeBoundaryValue::PostgresPool(value) => store
            .insert_for_owner(caller_process_id, ResourceValue::PostgresPool(value))
            .map(NativeBoundaryBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        NativeBoundaryValue::PostgresRow(value) => store
            .insert_for_owner(caller_process_id, ResourceValue::PostgresRow(value))
            .map(NativeBoundaryBridgeValue::Handle)
            .map_err(dispatch_resource_error),
        NativeBoundaryValue::PostgresRows(values) => values
            .into_iter()
            .map(|row| {
                store
                    .insert_for_owner(caller_process_id, ResourceValue::PostgresRow(row))
                    .map(NativeBoundaryBridgeValue::Handle)
                    .map_err(dispatch_resource_error)
            })
            .collect::<Result<Vec<_>, _>>()
            .map(NativeBoundaryBridgeValue::List),
        NativeBoundaryValue::OptionalPostgresRow(value) => value
            .map(|row| store.insert_for_owner(caller_process_id, ResourceValue::PostgresRow(row)))
            .transpose()
            .map(NativeBoundaryBridgeValue::OptionalHandle)
            .map_err(dispatch_resource_error),
        NativeBoundaryValue::PostgresConfig(_) | NativeBoundaryValue::JsonList(_) => {
            Err(DispatchError::new(
                "dispatch.postgres_requires_runtime_bridge",
                "Postgres input-only values cannot be returned across the runtime bridge.",
                0,
            ))
        }
        NativeBoundaryValue::OptionalText(value) => {
            Ok(NativeBoundaryBridgeValue::OptionalText(value))
        }
        NativeBoundaryValue::OptionalPath(value) => value
            .map(|path| store.insert_for_owner(caller_process_id, ResourceValue::Path(path)))
            .transpose()
            .map(NativeBoundaryBridgeValue::OptionalHandle)
            .map_err(dispatch_resource_error),
    }
}
