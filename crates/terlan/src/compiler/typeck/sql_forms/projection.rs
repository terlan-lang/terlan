use std::collections::HashSet;

use sqlparser::ast::{Expr, Ident, Query, SelectItem, SetExpr, Statement};

/// Stable failures produced while deriving SQL output names from the AST.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SqlProjectionError {
    DuplicateOutputName(String),
}

impl SqlProjectionError {
    /// Returns stable diagnostic text for projection-shape failures.
    pub(crate) fn message(&self) -> String {
        match self {
            Self::DuplicateOutputName(name) => {
                format!("SQL projection contains duplicate output name `{name}`")
            }
        }
    }
}

/// Derives conservative row field names from a parsed PostgreSQL statement.
///
/// Aliased expressions and direct column references have compiler-visible
/// names. Wildcards and unaliased expressions remain unknown until
/// Postgres-backed validation resolves the live schema and output descriptor.
pub(crate) fn statement_projection_fields(
    statement: &Statement,
) -> Result<Option<Vec<String>>, SqlProjectionError> {
    let items = match statement {
        Statement::Query(query) => query_projection(query),
        Statement::Insert(insert) => insert.returning.as_deref(),
        Statement::Update(update) => update.returning.as_deref(),
        Statement::Delete(delete) => delete.returning.as_deref(),
        _ => None,
    };
    let Some(items) = items else {
        return Ok(None);
    };

    projection_fields(items)
}

fn query_projection(query: &Query) -> Option<&[SelectItem]> {
    match query.body.as_ref() {
        SetExpr::Select(select) => Some(&select.projection),
        SetExpr::Query(query) => query_projection(query),
        _ => None,
    }
}

fn projection_fields(items: &[SelectItem]) -> Result<Option<Vec<String>>, SqlProjectionError> {
    let mut fields = Vec::with_capacity(items.len());
    let mut seen = HashSet::with_capacity(items.len());
    let mut complete = true;

    for item in items {
        let Some(field) = projection_field_name(item) else {
            complete = false;
            continue;
        };
        if !seen.insert(field.clone()) {
            return Err(SqlProjectionError::DuplicateOutputName(field));
        }
        fields.push(field);
    }

    if complete && !fields.is_empty() {
        Ok(Some(fields))
    } else {
        Ok(None)
    }
}

fn projection_field_name(item: &SelectItem) -> Option<String> {
    match item {
        SelectItem::ExprWithAlias { alias, .. } => Some(postgres_identifier_name(alias)),
        SelectItem::UnnamedExpr(Expr::Identifier(identifier)) => {
            Some(postgres_identifier_name(identifier))
        }
        SelectItem::UnnamedExpr(Expr::CompoundIdentifier(identifiers)) => {
            identifiers.last().map(postgres_identifier_name)
        }
        SelectItem::UnnamedExpr(_)
        | SelectItem::QualifiedWildcard(_, _)
        | SelectItem::Wildcard(_) => None,
    }
}

fn postgres_identifier_name(identifier: &Ident) -> String {
    if identifier.quote_style.is_some() {
        identifier.value.clone()
    } else {
        identifier.value.to_ascii_lowercase()
    }
}
