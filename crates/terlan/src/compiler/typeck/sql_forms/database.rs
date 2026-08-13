use std::collections::HashSet;

use sqlparser::ast::{
    Expr, Ident, ObjectName, Query, Select, SelectItem, SelectItemQualifiedWildcardKind, SetExpr,
    Statement, TableFactor,
};

use crate::database_schema::{DatabaseSchemaSnapshot, SchemaColumn, SchemaRelation};

/// One database-resolved output column in projection order.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct SqlDatabaseProjectionColumn {
    pub(crate) output_name: String,
    pub(crate) source_column: SchemaColumn,
}

/// Stable database-authoritative SQL validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SqlDatabaseValidationError {
    UnknownRelation(String),
    UnknownColumn { relation: String, column: String },
    UnknownQualifier(String),
    DuplicateOutputName(String),
}

impl SqlDatabaseValidationError {
    pub(crate) fn message(&self) -> String {
        match self {
            Self::UnknownRelation(relation) => {
                format!("SQL query references unknown relation `{relation}`")
            }
            Self::UnknownColumn { relation, column } => {
                format!("SQL query references unknown column `{column}` on relation `{relation}`")
            }
            Self::UnknownQualifier(qualifier) => {
                format!("SQL query references unknown relation qualifier `{qualifier}`")
            }
            Self::DuplicateOutputName(name) => {
                format!("SQL projection contains duplicate output name `{name}`")
            }
        }
    }
}

/// Resolves a simple PostgreSQL SELECT projection against a verified snapshot.
///
/// Complex joins, derived tables, set operations, and unaliased computed
/// expressions deliberately return `None`: their result descriptors require a
/// live Postgres prepare/describe boundary. A single physical relation is
/// validated completely, including quoted identifiers, aliases, direct column
/// references, and wildcard expansion in catalog ordinal order.
pub(crate) fn statement_schema_projection(
    statement: &Statement,
    snapshot: &DatabaseSchemaSnapshot,
) -> Result<Option<Vec<SqlDatabaseProjectionColumn>>, SqlDatabaseValidationError> {
    let Statement::Query(query) = statement else {
        return Ok(None);
    };
    query_schema_projection(query, snapshot)
}

fn query_schema_projection(
    query: &Query,
    snapshot: &DatabaseSchemaSnapshot,
) -> Result<Option<Vec<SqlDatabaseProjectionColumn>>, SqlDatabaseValidationError> {
    match query.body.as_ref() {
        SetExpr::Select(select) => select_schema_projection(select, snapshot),
        SetExpr::Query(query) => query_schema_projection(query, snapshot),
        _ => Ok(None),
    }
}

fn select_schema_projection(
    select: &Select,
    snapshot: &DatabaseSchemaSnapshot,
) -> Result<Option<Vec<SqlDatabaseProjectionColumn>>, SqlDatabaseValidationError> {
    let [from] = select.from.as_slice() else {
        return Ok(None);
    };
    if !from.joins.is_empty() {
        return Ok(None);
    }
    let TableFactor::Table {
        name, alias, args, ..
    } = &from.relation
    else {
        return Ok(None);
    };
    if args.is_some() {
        return Ok(None);
    }

    let Some((schema, relation_name)) = relation_identity(name) else {
        return Ok(None);
    };
    let relation = snapshot.relation(&schema, &relation_name).ok_or_else(|| {
        SqlDatabaseValidationError::UnknownRelation(format!("{schema}.{relation_name}"))
    })?;
    let alias = alias
        .as_ref()
        .map(|alias| postgres_identifier_name(&alias.name));
    projection_fields(select, relation, alias.as_deref())
}

fn projection_fields(
    select: &Select,
    relation: &SchemaRelation,
    alias: Option<&str>,
) -> Result<Option<Vec<SqlDatabaseProjectionColumn>>, SqlDatabaseValidationError> {
    let mut fields = Vec::new();
    let mut seen = HashSet::new();
    for item in &select.projection {
        match item {
            SelectItem::Wildcard(_) => append_wildcard(relation, &mut fields, &mut seen)?,
            SelectItem::QualifiedWildcard(qualifier, _) => {
                let SelectItemQualifiedWildcardKind::ObjectName(qualifier) = qualifier else {
                    return Ok(None);
                };
                validate_qualifier(qualifier, relation, alias)?;
                append_wildcard(relation, &mut fields, &mut seen)?;
            }
            SelectItem::UnnamedExpr(expression) => {
                let Some(column) = direct_column(expression, relation, alias)? else {
                    return Ok(None);
                };
                append_output(column.name.clone(), column, &mut fields, &mut seen)?;
            }
            SelectItem::ExprWithAlias {
                expr,
                alias: output,
            } => {
                let Some(column) = direct_column(expr, relation, alias)? else {
                    return Ok(None);
                };
                append_output(
                    postgres_identifier_name(output),
                    column,
                    &mut fields,
                    &mut seen,
                )?;
            }
        }
    }
    Ok((!fields.is_empty()).then_some(fields))
}

fn direct_column<'a>(
    expression: &Expr,
    relation: &'a SchemaRelation,
    alias: Option<&str>,
) -> Result<Option<&'a SchemaColumn>, SqlDatabaseValidationError> {
    let column = match expression {
        Expr::Identifier(column) => postgres_identifier_name(column),
        Expr::CompoundIdentifier(parts) if parts.len() == 2 => {
            let qualifier = postgres_identifier_name(&parts[0]);
            validate_qualifier_name(&qualifier, relation, alias)?;
            postgres_identifier_name(&parts[1])
        }
        Expr::CompoundIdentifier(_) => return Ok(None),
        _ => return Ok(None),
    };
    relation
        .columns
        .iter()
        .find(|item| item.name == column)
        .map(Some)
        .ok_or_else(|| SqlDatabaseValidationError::UnknownColumn {
            relation: format!("{}.{}", relation.schema, relation.name),
            column,
        })
}

fn append_wildcard(
    relation: &SchemaRelation,
    fields: &mut Vec<SqlDatabaseProjectionColumn>,
    seen: &mut HashSet<String>,
) -> Result<(), SqlDatabaseValidationError> {
    let mut columns = relation.columns.iter().collect::<Vec<_>>();
    columns.sort_by_key(|column| column.ordinal);
    for column in columns {
        append_output(column.name.clone(), column, fields, seen)?;
    }
    Ok(())
}

fn append_output(
    output: String,
    source_column: &SchemaColumn,
    fields: &mut Vec<SqlDatabaseProjectionColumn>,
    seen: &mut HashSet<String>,
) -> Result<(), SqlDatabaseValidationError> {
    if !seen.insert(output.clone()) {
        return Err(SqlDatabaseValidationError::DuplicateOutputName(output));
    }
    fields.push(SqlDatabaseProjectionColumn {
        output_name: output,
        source_column: source_column.clone(),
    });
    Ok(())
}

fn relation_identity(name: &ObjectName) -> Option<(String, String)> {
    let parts = name
        .0
        .iter()
        .map(|part| part.as_ident().map(postgres_identifier_name))
        .collect::<Option<Vec<_>>>()?;
    match parts.as_slice() {
        [relation] => Some(("public".to_string(), relation.clone())),
        [schema, relation] => Some((schema.clone(), relation.clone())),
        _ => None,
    }
}

fn validate_qualifier(
    qualifier: &ObjectName,
    relation: &SchemaRelation,
    alias: Option<&str>,
) -> Result<(), SqlDatabaseValidationError> {
    let parts = qualifier
        .0
        .iter()
        .map(|part| part.as_ident().map(postgres_identifier_name))
        .collect::<Option<Vec<_>>>();
    let Some(parts) = parts else {
        return Err(SqlDatabaseValidationError::UnknownQualifier(
            qualifier.to_string(),
        ));
    };
    let qualifier = parts.join(".");
    let valid = alias == Some(qualifier.as_str())
        || qualifier == relation.name
        || qualifier == format!("{}.{}", relation.schema, relation.name);
    if valid {
        Ok(())
    } else {
        Err(SqlDatabaseValidationError::UnknownQualifier(qualifier))
    }
}

fn validate_qualifier_name(
    qualifier: &str,
    relation: &SchemaRelation,
    alias: Option<&str>,
) -> Result<(), SqlDatabaseValidationError> {
    if alias == Some(qualifier) || qualifier == relation.name {
        Ok(())
    } else {
        Err(SqlDatabaseValidationError::UnknownQualifier(
            qualifier.to_string(),
        ))
    }
}

fn postgres_identifier_name(identifier: &Ident) -> String {
    if identifier.quote_style.is_some() {
        identifier.value.clone()
    } else {
        identifier.value.to_ascii_lowercase()
    }
}

#[cfg(test)]
#[path = "database_test.rs"]
#[cfg(test)]
mod database_test;
