use sqlparser::{ast::Statement, dialect::PostgreSqlDialect, parser::Parser};

/// Stable failures produced by the maintained PostgreSQL syntax boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SqlSyntaxValidationError {
    Malformed,
    MissingStatement,
    MultipleStatements,
}

impl SqlSyntaxValidationError {
    /// Returns the stable public diagnostic text for a SQL syntax failure.
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Malformed => "SQL form contains malformed PostgreSQL syntax",
            Self::MissingStatement => {
                "SQL form must contain exactly one PostgreSQL statement, found none"
            }
            Self::MultipleStatements => {
                "SQL form must contain exactly one PostgreSQL statement, found multiple"
            }
        }
    }
}

/// Validates one bound SQL statement with the maintained PostgreSQL parser.
///
/// Terlan interpolation is replaced with positional parameters before this
/// boundary. The compiler never derives SQL validity from its interpolation or
/// cardinality scanners.
pub(crate) fn parse_single_postgres_statement(
    sql: &str,
) -> Result<Statement, SqlSyntaxValidationError> {
    let mut statements = Parser::parse_sql(&PostgreSqlDialect {}, sql)
        .map_err(|_error| SqlSyntaxValidationError::Malformed)?;
    match statements.len() {
        0 => Err(SqlSyntaxValidationError::MissingStatement),
        1 => Ok(statements.remove(0)),
        _ => Err(SqlSyntaxValidationError::MultipleStatements),
    }
}
