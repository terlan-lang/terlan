//! Shared neutral and resource-handle values, independent of operation routing.

use crate::terlan_native::{http, json, path, postgres, regex, uri};
use crate::terlan_native_boundary::handle::NativeBoundaryHandle;

/// Neutral value shape accepted and returned by NativeBoundary adapter dispatch.
#[derive(Clone, Debug, PartialEq)]
pub enum NativeBoundaryValue {
    /// Terlan `Unit`.
    Unit,
    /// Terlan `String`.
    Text(String),
    /// Terlan VM-owned `Bytes`.
    Bytes(Vec<u8>),
    /// Terlan `Int`.
    Int(i64),
    /// Terlan `Float`.
    Float(f64),
    /// Terlan `Bool`.
    Bool(bool),
    /// Terlan atom identity without a host-language enum escape hatch.
    Atom(String),
    /// Descriptor-checked Terlan record/constructor value.
    Record {
        /// Constructor or record name.
        name: String,
        /// Ordered named fields.
        fields: Vec<(String, NativeBoundaryValue)>,
    },
    /// Ordered recursively owned Terlan values.
    List(Vec<NativeBoundaryValue>),
    /// Ordered fixed-arity Terlan tuple fields.
    Tuple(Vec<NativeBoundaryValue>),
    /// Opaque `std.data.Json.Json`.
    Json(json::Json),
    /// Opaque compiled `std.regex.Regex.Regex`.
    Regex(regex::Regex),
    /// Opaque `std.http.Request.Request`.
    HttpRequest(http::Request),
    /// Opaque `std.http.Response.Response`.
    HttpResponse(http::Response),
    /// Opaque `std.http.Cookies.Jar`.
    HttpCookieJar(http::CookieJar),
    /// Opaque `std.io.Path.Path`.
    Path(path::Path),
    /// Opaque `std.net.Uri.Uri`.
    Uri(uri::Uri),
    /// Opaque `std.db.Postgres.Config`.
    PostgresConfig(postgres::Config),
    /// Opaque `std.db.Postgres.Pool`.
    PostgresPool(postgres::Pool),
    /// Opaque `std.db.Postgres.Row`.
    PostgresRow(postgres::Row),
    /// `List[std.data.Json.Json]` used for Postgres parameter values.
    JsonList(Vec<json::Json>),
    /// `List[std.db.Postgres.Row]` returned by Postgres query operations.
    PostgresRows(Vec<postgres::Row>),
    /// `Option[std.db.Postgres.Row]` returned by single-row Postgres queries.
    OptionalPostgresRow(Option<postgres::Row>),
    /// `Option[String]` for string component accessors.
    OptionalText(Option<String>),
    /// `Option[Path]` for path component accessors.
    OptionalPath(Option<path::Path>),
}

/// Bridge-facing value shape that carries opaque resources as handles.
#[derive(Clone, Debug, PartialEq)]
pub enum NativeBoundaryBridgeValue {
    /// Terlan `Unit`.
    Unit,
    /// Terlan `String`.
    Text(String),
    /// Terlan VM-owned `Bytes`.
    Bytes(Vec<u8>),
    /// Terlan `Int`.
    Int(i64),
    /// Terlan `Float`.
    Float(f64),
    /// Terlan `Bool`.
    Bool(bool),
    /// Terlan atom identity.
    Atom(String),
    /// Recursively owned Terlan record/constructor value.
    Record {
        /// Constructor or record name.
        name: String,
        /// Ordered named fields.
        fields: Vec<(String, NativeBoundaryBridgeValue)>,
    },
    /// Opaque resource handle for JSON, path, URI, or later native resources.
    Handle(NativeBoundaryHandle),
    /// Structured Postgres connection configuration for `connect`.
    PostgresConfig(postgres::Config),
    /// `Option[String]` for string component accessors.
    OptionalText(Option<String>),
    /// `Option[Handle]` for optional opaque resources such as path parents.
    OptionalHandle(Option<NativeBoundaryHandle>),
    /// Terlan list carrying bridge-facing values.
    List(Vec<NativeBoundaryBridgeValue>),
    /// Ordered fixed-arity fields, including generation-tagged resource handles.
    Tuple(Vec<NativeBoundaryBridgeValue>),
}
