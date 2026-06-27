use std::env;
use std::process::ExitCode;

use crate::terlan_native::json;
use crate::terlan_native::postgres;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

/// Executes the private compiler SQL runtime helper.
///
/// Inputs:
/// - `args`: `operation sql_base64 params_json_base64 projection_base64`.
///
/// Output:
/// - Exit success with a line-oriented response consumed by
///   `terlan_sql_runtime.erl`.
/// - Exit failure only for malformed helper invocation; database/query errors
///   are encoded as runtime `err` responses so generated BEAM callers receive a
///   normal `Result`.
///
/// Transformation:
/// - Decodes the private protocol, resolves the process database environment,
///   delegates execution to the maintained SafeNative Postgres adapter, and
///   serializes typed projected row values without exposing this protocol as a
///   public Terlan API.
pub(crate) fn run(args: &[String]) -> ExitCode {
    match run_inner(args) {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!("err");
            println!("{}", encode_text(&error));
            ExitCode::SUCCESS
        }
    }
}

/// Runs the decoded SQL helper request.
///
/// Inputs:
/// - `args`: encoded helper arguments after the private subcommand name.
///
/// Output:
/// - Encoded success payload for BEAM or a runtime error message.
///
/// Transformation:
/// - Decodes helper arguments, opens the configured Postgres pool, dispatches
///   the requested operation, and serializes the projected result.
fn run_inner(args: &[String]) -> Result<String, String> {
    let [operation, sql, params, projection] = args else {
        return Err(
            "terlc __sql-runtime expects operation, sql, params, and projection arguments"
                .to_string(),
        );
    };
    let sql = decode_text(sql)?;
    let params = decode_params(params)?;
    let projection = decode_projection(projection)?;
    let config = database_config()?;
    let pool = postgres::connect(&config).map_err(postgres_error)?;
    match operation.as_str() {
        "query_one" => {
            let row = postgres::query_one(&pool, &sql, &params).map_err(postgres_error)?;
            match row {
                Some(row) => Ok(format!("ok_one\n{}\n", encode_row(&row, &projection)?)),
                None => Ok("ok_none\n".to_string()),
            }
        }
        "query" => {
            let rows = postgres::query(&pool, &sql, &params).map_err(postgres_error)?;
            let mut output = String::from("ok_rows\n");
            for row in rows {
                output.push_str(&encode_row(&row, &projection)?);
                output.push('\n');
            }
            Ok(output)
        }
        "execute" => {
            let affected = postgres::execute(&pool, &sql, &params).map_err(postgres_error)?;
            Ok(format!("ok_int\n{affected}\n"))
        }
        other => Err(format!("unsupported SQL runtime operation `{other}`")),
    }
}

/// Builds the Postgres configuration from process environment.
///
/// Inputs:
/// - `TERLAN_DATABASE_URL`, or the `POSTGRES_*` environment variable set.
///
/// Output:
/// - Validated SafeNative Postgres config.
///
/// Transformation:
/// - Prefers a single URL and otherwise assembles a URL from conventional
///   Docker/Postgres environment variables.
fn database_config() -> Result<postgres::Config, String> {
    if let Ok(url) = env::var("TERLAN_DATABASE_URL") {
        return validated_database_config(url);
    }
    let host = env::var("POSTGRES_HOST")
        .map_err(|_| "TERLAN_DATABASE_URL or POSTGRES_HOST must be set".to_string())?;
    let user = env::var("POSTGRES_USER")
        .map_err(|_| "TERLAN_DATABASE_URL or POSTGRES_USER must be set".to_string())?;
    let password = env::var("POSTGRES_PASSWORD")
        .map_err(|_| "TERLAN_DATABASE_URL or POSTGRES_PASSWORD must be set".to_string())?;
    let database = env::var("POSTGRES_DB")
        .map_err(|_| "TERLAN_DATABASE_URL or POSTGRES_DB must be set".to_string())?;
    let port = env::var("POSTGRES_PORT").unwrap_or_else(|_| "5432".to_string());
    validated_database_config(format!(
        "postgres://{user}:{password}@{host}:{port}/{database}"
    ))
}

/// Validates a database URL and applies runtime pool bounds.
///
/// Inputs:
/// - `url`: Postgres connection URL.
///
/// Output:
/// - SafeNative Postgres config accepted by the adapter.
///
/// Transformation:
/// - Adds conservative pool limits and delegates URL validation to the
///   maintained Postgres adapter.
fn validated_database_config(url: String) -> Result<postgres::Config, String> {
    let config = postgres::Config::new(url).with_pool_limits(1, 4);
    postgres::validate_config(&config).map_err(postgres_error)?;
    Ok(config)
}

/// Decodes a base64 UTF-8 helper argument.
///
/// Inputs:
/// - `encoded`: base64-encoded text argument.
///
/// Output:
/// - Decoded UTF-8 string or a stable protocol error.
///
/// Transformation:
/// - Converts the private shell-safe transport encoding back to text.
fn decode_text(encoded: &str) -> Result<String, String> {
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|error| format!("invalid base64 SQL runtime argument: {error}"))?;
    String::from_utf8(bytes).map_err(|error| format!("SQL runtime argument is not UTF-8: {error}"))
}

/// Decodes query parameters from a base64 JSON array.
///
/// Inputs:
/// - `encoded`: base64-encoded JSON array.
///
/// Output:
/// - SafeNative JSON parameter list.
///
/// Transformation:
/// - Parses serde JSON and converts each value into the runtime JSON wrapper.
fn decode_params(encoded: &str) -> Result<Vec<json::Json>, String> {
    let text = decode_text(encoded)?;
    let value = serde_json::from_str::<serde_json::Value>(&text)
        .map_err(|error| format!("SQL runtime params are not valid JSON: {error}"))?;
    let serde_json::Value::Array(values) = value else {
        return Err("SQL runtime params must be a JSON array".to_string());
    };
    Ok(values.into_iter().map(json::Json::from_serde).collect())
}

/// Decodes the projected column list.
///
/// Inputs:
/// - `encoded`: base64-encoded newline-separated projection text.
///
/// Output:
/// - Ordered projected field names.
///
/// Transformation:
/// - Splits the private projection protocol into field identifiers.
fn decode_projection(encoded: &str) -> Result<Vec<String>, String> {
    let text = decode_text(encoded)?;
    if text.is_empty() {
        return Ok(Vec::new());
    }
    Ok(text.lines().map(str::to_string).collect())
}

/// Encodes one projected database row for the BEAM SQL runtime.
///
/// Inputs:
/// - `row`: SafeNative Postgres row.
/// - `projection`: ordered projected field names.
///
/// Output:
/// - Tab-separated field payloads in the private SQL runtime protocol.
///
/// Transformation:
/// - Encodes each requested field using typed prefixes.
fn encode_row(row: &postgres::Row, projection: &[String]) -> Result<String, String> {
    projection
        .iter()
        .map(|field| encode_field(row, field))
        .collect::<Result<Vec<_>, _>>()
        .map(|fields| fields.join("\t"))
}

/// Encodes one typed database field.
///
/// Inputs:
/// - `row`: SafeNative Postgres row.
/// - `field`: projected column name.
///
/// Output:
/// - Private protocol field payload with a type prefix.
///
/// Transformation:
/// - Attempts supported scalar and JSON decoders in deterministic order.
fn encode_field(row: &postgres::Row, field: &str) -> Result<String, String> {
    if let Ok(value) = postgres::int(row, field) {
        return Ok(format!("i:{value}"));
    }
    if let Ok(value) = postgres::r#bool(row, field) {
        return Ok(format!("b:{value}"));
    }
    if let Ok(value) = postgres::string(row, field) {
        return Ok(format!("s:{}", encode_text(&value)));
    }
    if let Ok(value) = postgres::json(row, field) {
        let text = json::stringify(&value).map_err(|error| error.message().to_string())?;
        return Ok(format!("j:{}", encode_text(&text)));
    }
    Err(format!(
        "SQL runtime could not decode projected column `{field}`"
    ))
}

/// Encodes text for the shell-safe SQL helper protocol.
///
/// Inputs:
/// - `value`: text to encode.
///
/// Output:
/// - Base64 encoded string.
///
/// Transformation:
/// - Converts UTF-8 bytes to base64 without adding separators.
fn encode_text(value: &str) -> String {
    STANDARD.encode(value.as_bytes())
}

/// Formats a SafeNative Postgres error for BEAM callers.
///
/// Inputs:
/// - `error`: adapter error from the maintained Postgres layer.
///
/// Output:
/// - Stable `code: message` text.
///
/// Transformation:
/// - Preserves adapter error codes while keeping the private helper protocol
///   line-oriented.
fn postgres_error(error: postgres::PostgresError) -> String {
    format!("{}: {}", error.code(), error.message())
}

#[cfg(test)]
#[path = "sql_runtime_test.rs"]
mod sql_runtime_test;
