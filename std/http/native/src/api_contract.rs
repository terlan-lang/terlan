//! Package-owned route identity and minimal OpenAPI projection.
//!
//! This model contains route identity, not request/response type schemas.
//! Source discovery belongs to tooling; it is not part of this package.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const API_CONTRACT_SCHEMA: &str = "terlan-api-contract-v1";
pub const OPENAPI_VERSION: &str = "3.1.0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiContract {
    pub schema: String,
    pub service: ApiService,
    pub routes: Vec<ApiRoute>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiService {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiRoute {
    pub method: String,
    pub path: String,
    pub handler: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenApiDocument {
    pub openapi: String,
    pub info: OpenApiInfo,
    pub paths: BTreeMap<String, BTreeMap<String, OpenApiOperation>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenApiInfo {
    pub title: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenApiOperation {
    #[serde(rename = "operationId")]
    pub operation_id: String,
    pub responses: BTreeMap<String, OpenApiResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenApiResponse {
    pub description: String,
}

impl ApiContract {
    pub fn empty(service_name: impl Into<String>, version: impl Into<String>) -> Self {
        Self::from_routes(service_name, version, Vec::new())
    }

    /// Normalizes route ordering independently of how tooling discovers routes.
    pub fn from_routes(
        service_name: impl Into<String>,
        version: impl Into<String>,
        mut routes: Vec<ApiRoute>,
    ) -> Self {
        routes.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then(left.method.cmp(&right.method))
                .then(left.handler.cmp(&right.handler))
        });
        Self {
            schema: API_CONTRACT_SCHEMA.to_string(),
            service: ApiService {
                name: service_name.into(),
                version: version.into(),
            },
            routes,
        }
    }

    /// Projects route identities without inventing request or response schemas.
    pub fn to_openapi(&self) -> OpenApiDocument {
        let mut paths = BTreeMap::<String, BTreeMap<String, OpenApiOperation>>::new();
        for route in &self.routes {
            let operation = OpenApiOperation {
                operation_id: openapi_operation_id(route),
                responses: BTreeMap::from([(
                    "200".to_string(),
                    OpenApiResponse {
                        description: "Successful response".to_string(),
                    },
                )]),
            };
            paths
                .entry(openapi_path(&route.path))
                .or_default()
                .insert(route.method.to_lowercase(), operation);
        }
        OpenApiDocument {
            openapi: OPENAPI_VERSION.to_string(),
            info: OpenApiInfo {
                title: self.service.name.clone(),
                version: self.service.version.clone(),
            },
            paths,
        }
    }
}

/// Converts a Terlan router path into OpenAPI path syntax.
fn openapi_path(path: &str) -> String {
    let normalized = if path == "*" { "/*" } else { path };
    normalized
        .split('/')
        .map(|segment| {
            if let Some(param) = segment.strip_prefix(':') {
                format!("{{{param}}}")
            } else {
                segment.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Builds a deterministic OpenAPI operation id.
fn openapi_operation_id(route: &ApiRoute) -> String {
    let handler = route.handler.replace('.', "_");
    format!("{}_{}", route.method.to_lowercase(), handler)
}

#[cfg(test)]
#[path = "api_contract_test.rs"]
mod tests;
