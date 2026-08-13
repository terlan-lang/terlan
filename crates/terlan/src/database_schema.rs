//! Shared, deterministic database schema evidence.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(crate) const DATABASE_SCHEMA_SNAPSHOT_SCHEMA: &str = "terlan.db-schema-snapshot.v1";

/// Canonical schema snapshot persisted beside project migrations.
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DatabaseSchemaSnapshot {
    pub(crate) schema: String,
    pub(crate) database_product: String,
    pub(crate) migration_snapshot_id: String,
    pub(crate) schema_fingerprint: String,
    pub(crate) relations: Vec<SchemaRelation>,
    pub(crate) enums: Vec<SchemaEnum>,
}

/// One user-visible table or view.
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SchemaRelation {
    pub(crate) schema: String,
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) columns: Vec<SchemaColumn>,
    pub(crate) constraints: Vec<SchemaConstraint>,
    pub(crate) indexes: Vec<SchemaIndex>,
}

/// One relation column with database-authoritative type/nullability metadata.
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SchemaColumn {
    pub(crate) name: String,
    pub(crate) ordinal: i64,
    pub(crate) data_type: String,
    pub(crate) user_type_schema: String,
    pub(crate) user_type_name: String,
    pub(crate) nullable: bool,
    pub(crate) default: Option<String>,
    pub(crate) identity: bool,
    pub(crate) identity_generation: Option<String>,
    pub(crate) generated: bool,
    pub(crate) generation_expression: Option<String>,
}

/// Typed VM row codec shared by schema validation and native decoding.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum DatabaseColumnCodec {
    Binary,
    Bool,
    Int,
    Json,
}

impl DatabaseColumnCodec {
    /// Resolves one stable Postgres catalog type to its Terlan row codec.
    pub(crate) fn for_schema_column(column: &SchemaColumn) -> Option<Self> {
        Self::resolve(
            Some((
                column.user_type_schema.as_str(),
                column.user_type_name.as_str(),
            )),
            None,
        )
    }

    /// Resolves either snapshot type identity or native OID through one table.
    pub(crate) fn resolve(
        schema_type: Option<(&str, &str)>,
        postgres_oid: Option<i64>,
    ) -> Option<Self> {
        match (schema_type, postgres_oid) {
            (Some(("pg_catalog", "bool")), None) | (None, Some(16)) => Some(Self::Bool),
            (Some(("pg_catalog", "int2" | "int4" | "int8" | "oid")), None)
            | (None, Some(20 | 21 | 23 | 26)) => Some(Self::Int),
            (Some(("pg_catalog", "json" | "jsonb")), None) | (None, Some(114 | 3802)) => {
                Some(Self::Json)
            }
            (Some(("pg_catalog", "bpchar" | "char" | "name" | "text" | "varchar")), None)
            | (None, Some(18 | 19 | 25 | 1042 | 1043)) => Some(Self::Binary),
            _ => None,
        }
    }

    /// Returns the canonical Terlan type spelling for diagnostics.
    pub(crate) const fn terlan_type_name(self) -> &'static str {
        match self {
            Self::Binary => "Binary",
            Self::Bool => "Bool",
            Self::Int => "Int",
            Self::Json => "Json",
        }
    }
}

impl SchemaColumn {
    /// Returns the qualified database type identity captured by the snapshot.
    pub(crate) fn qualified_database_type(&self) -> String {
        format!("{}.{}", self.user_type_schema, self.user_type_name)
    }
}

/// One canonical Postgres constraint definition.
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SchemaConstraint {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) definition: String,
}

/// One canonical Postgres index definition.
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SchemaIndex {
    pub(crate) name: String,
    pub(crate) definition: String,
}

/// One user-defined Postgres enum and its ordered labels.
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SchemaEnum {
    pub(crate) schema: String,
    pub(crate) name: String,
    pub(crate) labels: Vec<String>,
}

impl DatabaseSchemaSnapshot {
    /// Loads and verifies one persisted schema snapshot.
    pub(crate) fn load(path: &Path) -> Result<Self, terlan_runtime_abi::BoundaryError> {
        let text = fs::read_to_string(path).map_err(|error| {
            terlan_runtime_abi::BoundaryError::sourced(
                terlan_runtime_abi::ErrorDomain::CompilerPhase,
                "db.snapshot.read",
                "DatabaseSchemaSnapshot::load",
                format!(
                    "error[db.snapshot.read]: Cannot read schema snapshot `{}`: {error}.",
                    path.display()
                ),
                error,
            )
        })?;
        let snapshot = serde_json::from_str::<Self>(&text).map_err(|error| {
            database_schema_error(
                "DatabaseSchemaSnapshot::load",
                snapshot_corrupt_message(path, &format!("malformed JSON: {error}")),
            )
        })?;
        snapshot.validate_integrity(path)?;
        Ok(snapshot)
    }

    /// Finds and loads the nearest project schema snapshot for a source file.
    pub(crate) fn discover_for_source(
        source_path: &Path,
    ) -> Result<Option<Self>, terlan_runtime_abi::BoundaryError> {
        let start = if source_path.is_dir() {
            source_path
        } else {
            source_path.parent().unwrap_or_else(|| Path::new("."))
        };
        for ancestor in start.ancestors() {
            let candidates = [
                ancestor.join("db/schema.snapshot.json"),
                ancestor.join("schema.snapshot.json"),
            ];
            let present = candidates
                .iter()
                .filter(|candidate| candidate.is_file())
                .collect::<Vec<_>>();
            match present.as_slice() {
                [] => {}
                [path] => return Self::load(path).map(Some),
                _ => {
                    return Err(database_schema_error(
                        "DatabaseSchemaSnapshot::discover_for_source",
                        format!(
                            "error[db.snapshot.ambiguous]: Multiple schema snapshots are visible from `{}`: `{}` and `{}`.",
                            source_path.display(),
                            candidates[0].display(),
                            candidates[1].display()
                        ),
                    ));
                }
            }
        }
        Ok(None)
    }

    /// Returns one exact schema relation.
    pub(crate) fn relation(&self, schema: &str, name: &str) -> Option<&SchemaRelation> {
        self.relations
            .iter()
            .find(|relation| relation.schema == schema && relation.name == name)
    }

    pub(crate) fn validate_integrity(
        &self,
        path: &Path,
    ) -> Result<(), terlan_runtime_abi::BoundaryError> {
        if self.schema != DATABASE_SCHEMA_SNAPSHOT_SCHEMA {
            return Err(database_schema_error(
                "DatabaseSchemaSnapshot::validate_integrity",
                unsupported_snapshot_contract_message(path, "schema version"),
            ));
        }
        if self.database_product != "PostgreSQL" {
            return Err(database_schema_error(
                "DatabaseSchemaSnapshot::validate_integrity",
                unsupported_snapshot_contract_message(path, "database product"),
            ));
        }
        let actual = schema_fingerprint(&self.relations, &self.enums)?;
        if self.schema_fingerprint != actual {
            return Err(database_schema_error(
                "DatabaseSchemaSnapshot::validate_integrity",
                snapshot_corrupt_message(
                    path,
                    "stored schema fingerprint does not match snapshot contents",
                ),
            ));
        }
        Ok(())
    }
}

pub(crate) fn schema_fingerprint(
    relations: &[SchemaRelation],
    enums: &[SchemaEnum],
) -> Result<String, terlan_runtime_abi::BoundaryError> {
    let canonical = serde_json::to_vec(&(relations, enums)).map_err(|error| {
        terlan_runtime_abi::BoundaryError::sourced(
            terlan_runtime_abi::ErrorDomain::CompilerPhase,
            "db.snapshot.canonicalize",
            "schema_fingerprint",
            format!("cannot canonicalize database schema: {error}"),
            error,
        )
    })?;
    let digest = Sha256::digest(canonical);
    Ok(prefixed_digest(&digest))
}

fn database_schema_error(
    operation: &'static str,
    rendered: String,
) -> terlan_runtime_abi::BoundaryError {
    terlan_runtime_abi::BoundaryError::message(
        terlan_runtime_abi::ErrorDomain::CompilerPhase,
        operation,
        rendered,
    )
}

pub(crate) fn prefixed_digest(bytes: &[u8]) -> String {
    let digest = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sha256:{digest}")
}

fn unsupported_snapshot_contract_message(path: &Path, field: &str) -> String {
    format!(
        "error[db.snapshot.unsupported_contract]: Schema snapshot `{}` uses an unsupported {field}.",
        path.display()
    )
}

fn snapshot_corrupt_message(path: &Path, reason: &str) -> String {
    format!(
        "error[db.snapshot.corrupt]: Schema snapshot `{}` is corrupt: {reason}.",
        path.display()
    )
}

#[cfg(test)]
#[path = "database_schema_test.rs"]
#[cfg(test)]
mod database_schema_test;
