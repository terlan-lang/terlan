use sqlparser::ast::{Expr, LimitClause, Query, Set, Statement, Value};

use super::SqlCardinality;

/// Compiler-owned operation class derived from the maintained SQL AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SqlQueryKind {
    Select,
    Insert,
    Update,
    Delete,
    Ddl,
    Transaction,
    Other,
}

/// Transaction context required to execute a validated SQL statement safely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SqlTransactionRequirement {
    AutocommitAllowed,
    ActiveTransactionRequired,
    VmManagedControl,
}

impl SqlTransactionRequirement {
    /// Returns the stable label carried by diagnostics and CoreIR contracts.
    pub(crate) fn as_diagnostic_label(self) -> &'static str {
        match self {
            Self::AutocommitAllowed => "autocommit_allowed",
            Self::ActiveTransactionRequired => "active_transaction_required",
            Self::VmManagedControl => "vm_managed_control",
        }
    }

    /// Returns the wrapper blocker for transaction control owned by the VM.
    pub(crate) fn wrapper_blocker(self) -> Option<&'static str> {
        (self == Self::VmManagedControl).then_some(
            "SQL transaction control is VM-owned; use the typed database transaction API",
        )
    }
}

impl SqlQueryKind {
    /// Returns the stable label used by diagnostics and validation reports.
    pub(crate) fn as_diagnostic_label(self) -> &'static str {
        match self {
            Self::Select => "select",
            Self::Insert => "insert",
            Self::Update => "update",
            Self::Delete => "delete",
            Self::Ddl => "ddl",
            Self::Transaction => "transaction",
            Self::Other => "other",
        }
    }
}

/// Classifies a parsed PostgreSQL statement without inspecting rendered SQL.
pub(crate) fn classify_statement(statement: &Statement) -> SqlQueryKind {
    match statement {
        Statement::Query(_) => SqlQueryKind::Select,
        Statement::Insert(_) => SqlQueryKind::Insert,
        Statement::Update(_) => SqlQueryKind::Update,
        Statement::Delete(_) => SqlQueryKind::Delete,
        Statement::StartTransaction { .. }
        | Statement::Commit { .. }
        | Statement::Rollback { .. }
        | Statement::Savepoint { .. }
        | Statement::ReleaseSavepoint { .. }
        | Statement::Set(Set::SetTransaction { .. }) => SqlQueryKind::Transaction,
        Statement::Truncate(_)
        | Statement::CreateView(_)
        | Statement::CreateTable(_)
        | Statement::CreateVirtualTable { .. }
        | Statement::CreateIndex(_)
        | Statement::CreateRole(_)
        | Statement::CreateServer(_)
        | Statement::CreatePolicy { .. }
        | Statement::CreateOperator(_)
        | Statement::CreateOperatorFamily(_)
        | Statement::CreateOperatorClass(_)
        | Statement::AlterTable(_)
        | Statement::AlterSchema(_)
        | Statement::AlterIndex { .. }
        | Statement::AlterView { .. }
        | Statement::AlterType(_)
        | Statement::AlterRole { .. }
        | Statement::AlterPolicy { .. }
        | Statement::Drop { .. }
        | Statement::DropFunction(_)
        | Statement::DropDomain(_)
        | Statement::DropProcedure { .. }
        | Statement::DropPolicy { .. }
        | Statement::CreateExtension(_)
        | Statement::DropExtension(_)
        | Statement::DropOperator(_)
        | Statement::DropOperatorFamily(_)
        | Statement::DropOperatorClass(_)
        | Statement::CreateSchema { .. }
        | Statement::CreateDatabase { .. }
        | Statement::CreateFunction(_)
        | Statement::CreateTrigger(_)
        | Statement::DropTrigger(_)
        | Statement::CreateProcedure { .. } => SqlQueryKind::Ddl,
        _ => SqlQueryKind::Other,
    }
}

/// Derives the transaction context requirement from the parsed SQL AST.
pub(crate) fn statement_transaction_requirement(
    statement: &Statement,
) -> SqlTransactionRequirement {
    match statement {
        Statement::Query(query) if !query.locks.is_empty() => {
            SqlTransactionRequirement::ActiveTransactionRequired
        }
        Statement::Savepoint { .. } | Statement::ReleaseSavepoint { .. } => {
            SqlTransactionRequirement::ActiveTransactionRequired
        }
        Statement::Rollback {
            savepoint: Some(_), ..
        } => SqlTransactionRequirement::ActiveTransactionRequired,
        Statement::StartTransaction { .. }
        | Statement::Commit { .. }
        | Statement::Rollback {
            savepoint: None, ..
        }
        | Statement::Set(Set::SetTransaction { .. }) => SqlTransactionRequirement::VmManagedControl,
        _ => SqlTransactionRequirement::AutocommitAllowed,
    }
}

/// Derives conservative result cardinality from a parsed statement.
pub(crate) fn statement_cardinality(statement: &Statement) -> SqlCardinality {
    match statement {
        Statement::Query(query) => {
            if query_returns_at_most_one(query) {
                SqlCardinality::OptionalOne
            } else {
                SqlCardinality::ManyRows
            }
        }
        Statement::Insert(insert) if insert.returning.is_some() => SqlCardinality::ManyRows,
        Statement::Update(update) if update.returning.is_some() => SqlCardinality::ManyRows,
        Statement::Delete(delete) if delete.returning.is_some() => SqlCardinality::ManyRows,
        Statement::Insert(_) | Statement::Update(_) | Statement::Delete(_) => {
            SqlCardinality::AffectedRows
        }
        _ => SqlCardinality::Ambiguous,
    }
}

fn query_returns_at_most_one(query: &Query) -> bool {
    let limit_is_one = match &query.limit_clause {
        Some(LimitClause::LimitOffset {
            limit: Some(limit), ..
        }) => expression_is_one(limit),
        Some(LimitClause::OffsetCommaLimit { limit, .. }) => expression_is_one(limit),
        _ => false,
    };
    let fetch_is_one = query.fetch.as_ref().is_some_and(|fetch| {
        !fetch.with_ties && !fetch.percent && fetch.quantity.as_ref().is_none_or(expression_is_one)
    });
    limit_is_one || fetch_is_one
}

fn expression_is_one(expression: &Expr) -> bool {
    match expression {
        Expr::Value(value) => matches!(&value.value, Value::Number(number, _) if number == "1"),
        Expr::Nested(inner) => expression_is_one(inner),
        _ => false,
    }
}
