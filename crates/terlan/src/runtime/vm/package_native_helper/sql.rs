//! Trusted-service execution for compiler-owned typed SQL forms.

use std::fmt;

use crate::runtime::vm::postgres::{VmPostgresDecodedValue, VmPostgresRow};
use crate::runtime::vm::postgres_command::VmPostgresCommandClient;
use crate::runtime::vm::pure_native::PureNativeCapabilityRequest;
use crate::runtime::vm::VmRuntimeResult;
use crate::terlan_native::{json, postgres};
use crate::terlan_native_boundary::term::{NativeBoundaryReplyTerm, NativeBoundaryTerm};

#[derive(Debug)]
struct SqlCapabilityError(String);

type SqlCapabilityResult<T> = Result<T, SqlCapabilityError>;

impl fmt::Display for SqlCapabilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for SqlCapabilityError {}

impl From<String> for SqlCapabilityError {
    fn from(message: String) -> Self {
        Self(message)
    }
}

impl From<&str> for SqlCapabilityError {
    fn from(message: &str) -> Self {
        Self(message.to_owned())
    }
}

impl From<SqlCapabilityError> for String {
    fn from(error: SqlCapabilityError) -> Self {
        error.0
    }
}

impl From<SqlCapabilityError> for crate::runtime::vm::VmRuntimeError {
    fn from(error: SqlCapabilityError) -> Self {
        String::from(error).into()
    }
}

/// Executes the compiler-owned SQL capability for the trusted service runtime.
pub(super) fn dispatch(
    request: &PureNativeCapabilityRequest,
) -> Option<VmRuntimeResult<NativeBoundaryReplyTerm>> {
    if request.operation != "std.db.sql.query" {
        return None;
    }
    Some(execute(request).map(NativeBoundaryReplyTerm::Ok))
}

fn execute(request: &PureNativeCapabilityRequest) -> VmRuntimeResult<NativeBoundaryTerm> {
    let [NativeBoundaryTerm::Text(row_type), NativeBoundaryTerm::Text(statement), NativeBoundaryTerm::Text(_query_kind), NativeBoundaryTerm::Text(transaction_requirement), NativeBoundaryTerm::Text(cardinality), NativeBoundaryTerm::List(projection), parameters @ ..] =
        request.arguments.as_slice()
    else {
        return Err("error[sql.capability_arguments]: malformed typed SQL capability frame".into());
    };
    if transaction_requirement != "autocommit_allowed" {
        return Ok(sql_error(
            "transaction_required",
            "typed SQL operation requires VM-managed transaction control",
        ));
    }
    let projection = projection
        .iter()
        .map(|field| match field {
            NativeBoundaryTerm::Text(field) => Ok(field.clone()),
            _ => Err("error[sql.projection]: SQL projection fields must be String".to_string()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let parameters = parameters
        .iter()
        .map(sql_parameter)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error: SqlCapabilityError| error.to_string())?;
    let config = database_config().map_err(|error| error.to_string())?;
    let mut client = match VmPostgresCommandClient::connect(&config) {
        Ok(client) => client,
        Err(error) => return Ok(sql_error("database_unavailable", &error)),
    };
    match cardinality.as_str() {
        "optional_one" => match client.query_one(statement, parameters) {
            Ok(row) => Ok(result_ok(
                option_row(&mut client, row_type, row, &projection)
                    .map_err(|error| error.to_string())?,
            )),
            Err(error) => Ok(sql_error("query_failed", &error)),
        },
        "many_rows" => match client.query(statement, parameters) {
            Ok(rows) => Ok(result_ok(NativeBoundaryTerm::List(
                rows.into_iter()
                    .map(|row| row_term(&mut client, row_type, row, &projection))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error: SqlCapabilityError| error.to_string())?,
            ))),
            Err(error) => Ok(sql_error("query_failed", &error)),
        },
        "affected_rows" => match client.execute(statement, parameters) {
            Ok(affected) => Ok(result_ok(NativeBoundaryTerm::Int(affected))),
            Err(error) => Ok(sql_error("query_failed", &error)),
        },
        other => {
            Err(format!("error[sql.cardinality]: unsupported SQL cardinality `{other}`").into())
        }
    }
}

fn database_config() -> SqlCapabilityResult<postgres::Config> {
    let url = std::env::var("TERLAN_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .map_err(|_| "error[sql.configuration]: DATABASE_URL is not configured".to_string())?;
    let config = postgres::Config::new(url).with_pool_limits(1, 4);
    postgres::validate_config(&config)
        .map_err(|error| format!("error[{}]: {}", error.code(), error.message()))?;
    Ok(config)
}

fn sql_parameter(term: &NativeBoundaryTerm) -> SqlCapabilityResult<json::Json> {
    let value = match term {
        NativeBoundaryTerm::Text(value) => serde_json::Value::String(value.clone()),
        NativeBoundaryTerm::Int(value) => serde_json::Value::Number((*value).into()),
        NativeBoundaryTerm::Bool(value) => serde_json::Value::Bool(*value),
        NativeBoundaryTerm::Float(value) => serde_json::Number::from_f64(*value)
            .map(serde_json::Value::Number)
            .ok_or_else(|| "error[sql.parameter]: non-finite Float parameter".to_string())?,
        NativeBoundaryTerm::Atom(value) => serde_json::Value::String(value.clone()),
        NativeBoundaryTerm::Record { name, fields } if name == "None" && fields.is_empty() => {
            serde_json::Value::Null
        }
        NativeBoundaryTerm::Record { name, fields } if name == "Some" => {
            let Some((_, value)) = fields.first() else {
                return Err("error[sql.parameter]: malformed Some parameter".into());
            };
            return sql_parameter(value);
        }
        _ => return Err("error[sql.parameter]: unsupported typed SQL parameter".into()),
    };
    Ok(json::Json::from_serde(value))
}

fn option_row(
    client: &mut VmPostgresCommandClient,
    row_type: &str,
    row: Option<VmPostgresRow>,
    projection: &[String],
) -> SqlCapabilityResult<NativeBoundaryTerm> {
    match row {
        Some(row) => Ok(NativeBoundaryTerm::Record {
            name: "Some".to_string(),
            fields: vec![(
                "value".to_string(),
                row_term(client, row_type, row, projection)?,
            )],
        }),
        None => Ok(NativeBoundaryTerm::Record {
            name: "None".to_string(),
            fields: Vec::new(),
        }),
    }
}

fn row_term(
    client: &mut VmPostgresCommandClient,
    row_type: &str,
    row: VmPostgresRow,
    projection: &[String],
) -> SqlCapabilityResult<NativeBoundaryTerm> {
    let fields = projection
        .iter()
        .map(|field| {
            client
                .decode_dynamic(row, field)
                .map(decoded_term)
                .map(|value| (field.clone(), value))
                .map_err(|error| {
                    format!("error[sql.decode]: could not decode column `{field}`: {error}")
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(NativeBoundaryTerm::Record {
        name: row_type.rsplit('.').next().unwrap_or(row_type).to_string(),
        fields,
    })
}

fn decoded_term(value: VmPostgresDecodedValue) -> NativeBoundaryTerm {
    match value {
        VmPostgresDecodedValue::Null => NativeBoundaryTerm::Record {
            name: "None".to_string(),
            fields: Vec::new(),
        },
        VmPostgresDecodedValue::String(value) | VmPostgresDecodedValue::Json(value) => {
            NativeBoundaryTerm::Text(value)
        }
        VmPostgresDecodedValue::Int(value) => NativeBoundaryTerm::Int(value),
        VmPostgresDecodedValue::Bool(value) => NativeBoundaryTerm::Bool(value),
    }
}

fn result_ok(value: NativeBoundaryTerm) -> NativeBoundaryTerm {
    NativeBoundaryTerm::Record {
        name: "Ok".to_string(),
        fields: vec![("value".to_string(), value)],
    }
}

fn sql_error(code: &str, message: &str) -> NativeBoundaryTerm {
    eprintln!("terlan trusted SQL capability error[{code}]: {message}");
    NativeBoundaryTerm::Record {
        name: "Err".to_string(),
        fields: vec![(
            "reason".to_string(),
            NativeBoundaryTerm::Record {
                name: "Error".to_string(),
                fields: vec![
                    (
                        "code".to_string(),
                        NativeBoundaryTerm::Atom("unknown".to_string()),
                    ),
                    (
                        "message".to_string(),
                        NativeBoundaryTerm::Text(format!("{code}: {message}")),
                    ),
                ],
            },
        )],
    }
}
