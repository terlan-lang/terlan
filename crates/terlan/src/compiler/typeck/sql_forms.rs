use crate::terlan_syntax::{SyntaxExprKind, SyntaxExprOutput};

pub(crate) mod projection;
mod scanner;
pub(crate) use projection::simple_sql_projection_fields;
use scanner::{
    copy_sql_block_comment, copy_sql_line_comment, copy_sql_quoted_segment,
    read_sql_interpolation_source, sql_words_without_literals_or_comments,
};

/// Cardinality shape inferred for a compiler-known SQL form.
///
/// Inputs:
/// - Preserved SQL text from `sql[Row] { ... }`.
///
/// Output:
/// - The result wrapper shape the compiler can infer without schema access.
///
/// Transformation:
/// - Represents only conservative source-shape facts. Database-backed
///   validation remains responsible for authoritative SQL semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SqlCardinality {
    OptionalOne,
    ManyRows,
    AffectedRows,
    Ambiguous,
}

/// Bound SQL text plus parameter count for a compiler-known SQL form.
///
/// Inputs:
/// - Raw SQL text containing Terlan `${expr}` interpolation islands.
///
/// Output:
/// - SQL text where each interpolation is replaced by a Postgres positional
///   parameter placeholder plus the number of generated placeholders.
///
/// Transformation:
/// - Keeps SQL text, comments, and string literals intact while replacing only
///   unquoted interpolation islands with `$1`, `$2`, and so on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SqlParameterBinding {
    pub(crate) sql: String,
    pub(crate) parameter_count: usize,
}

/// Compiler-facing analysis for a `sql[Row] { ... }` form.
///
/// Inputs:
/// - Syntax-output raw macro expression named `sql`.
///
/// Output:
/// - Row type metadata, rewritten SQL parameter binding, and inferred
///   cardinality in one stable payload for later wrapper lowering.
///
/// Transformation:
/// - Combines syntax-output type arguments, raw SQL text, conservative
///   cardinality inference, and interpolation placeholder binding without
///   validating SQL semantics against a live database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SqlFormAnalysis {
    pub(crate) row_type: Option<String>,
    pub(crate) row_type_arg_count: usize,
    pub(crate) binding: SqlParameterBinding,
    pub(crate) cardinality: SqlCardinality,
    pub(crate) result_type: Option<String>,
}

/// Compiler-facing wrapper plan for a ready SQL form.
///
/// Inputs:
/// - A syntax-output `sql[Row] { ... }` expression that passed first-release
///   wrapper-front-door checks.
///
/// Output:
/// - Bound SQL, parameter count, row type, inferred cardinality, result type,
///   and optional simple projection fields.
///
/// Transformation:
/// - Freezes the SQL analysis data needed by generated query-wrapper emission
///   without yet choosing a backend function name or runtime call sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SqlWrapperPlan {
    pub(crate) row_type: String,
    pub(crate) bound_sql: String,
    pub(crate) parameter_count: usize,
    pub(crate) cardinality: SqlCardinality,
    pub(crate) result_type: String,
    pub(crate) projection_fields: Option<Vec<String>>,
}

/// Error produced while building a SQL wrapper plan.
///
/// Inputs:
/// - SQL-form analysis and wrapper readiness checks.
///
/// Output:
/// - Stable reason why wrapper planning could not proceed.
///
/// Transformation:
/// - Distinguishes malformed SQL-form metadata from otherwise valid SQL forms
///   that are blocked by first-release wrapper prerequisites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SqlWrapperPlanError {
    Analysis(SqlFormAnalysisError),
    NotReady(Vec<String>),
    MissingResultType,
    MissingRowType,
}

impl SqlFormAnalysis {
    /// Returns the blocking reasons before SQL wrapper lowering may run.
    ///
    /// Inputs:
    /// - `self`: analyzed SQL-form metadata.
    /// - `parsed_expression_count`: number of interpolation expression children
    ///   preserved by syntax output.
    ///
    /// Output:
    /// - Empty vector when the form satisfies the current wrapper-front-door
    ///   requirements.
    /// - Stable blocking messages when row type arity, parameter binding, or
    ///   cardinality still prevents wrapper generation.
    ///
    /// Transformation:
    /// - Consolidates the first-release SQL lowering gate into one reusable
    ///   compiler decision so diagnostics and later wrapper generation do not
    ///   duplicate prerequisite checks.
    pub(crate) fn wrapper_lowering_blockers(&self, parsed_expression_count: usize) -> Vec<String> {
        let mut blockers = Vec::new();
        if let Some(message) = self.row_type_arity_message() {
            blockers.push(message);
        }
        if parsed_expression_count != self.binding.parameter_count {
            blockers.push(self.parameter_count_consistency_message(parsed_expression_count));
        }
        if let Some(message) = self.cardinality_requirement_message() {
            blockers.push(message);
        }
        blockers
    }

    /// Returns whether SQL wrapper lowering can proceed past syntax analysis.
    ///
    /// Inputs:
    /// - `self`: analyzed SQL-form metadata.
    /// - `parsed_expression_count`: number of interpolation expression children
    ///   preserved by syntax output.
    ///
    /// Output:
    /// - `true` when first-release wrapper prerequisites are satisfied.
    /// - `false` when row type arity, parameter binding, or cardinality is not
    ///   ready for wrapper lowering.
    ///
    /// Transformation:
    /// - Converts structured SQL-form analysis into a boolean gate for later
    ///   wrapper generation without performing database-backed validation.
    pub(crate) fn is_ready_for_wrapper_lowering(&self, parsed_expression_count: usize) -> bool {
        self.wrapper_lowering_blockers(parsed_expression_count)
            .is_empty()
    }

    /// Returns a stable diagnostic summary of SQL wrapper lowering readiness.
    ///
    /// Inputs:
    /// - `self`: analyzed SQL-form metadata.
    /// - `parsed_expression_count`: number of interpolation expression children
    ///   preserved by syntax output.
    ///
    /// Output:
    /// - Human-readable readiness message for unresolved SQL-form diagnostics.
    ///
    /// Transformation:
    /// - Renders the same prerequisite checks used by
    ///   `is_ready_for_wrapper_lowering` as a single diagnostic fragment.
    pub(crate) fn wrapper_lowering_readiness_message(
        &self,
        parsed_expression_count: usize,
    ) -> String {
        if self.is_ready_for_wrapper_lowering(parsed_expression_count) {
            "SQL wrapper lowering readiness: ready".to_string()
        } else {
            let blockers = self.wrapper_lowering_blockers(parsed_expression_count);
            format!(
                "SQL wrapper lowering readiness: blocked ({})",
                blockers.join("; ")
            )
        }
    }

    /// Returns a diagnostic when a SQL form does not declare exactly one row type.
    ///
    /// Inputs:
    /// - `self`: analyzed SQL-form metadata.
    ///
    /// Output:
    /// - `None` when exactly one row type argument is present; otherwise a
    ///   stable row-type arity message.
    ///
    /// Transformation:
    /// - Converts parser metadata into the first-release SQL-form requirement
    ///   that every compiler-known SQL form must use `sql[RowType] { ... }`.
    pub(crate) fn row_type_arity_message(&self) -> Option<String> {
        if self.row_type_arg_count == 1 {
            None
        } else {
            Some(format!(
                "SQL form requires exactly one explicit row type argument, found {}",
                self.row_type_arg_count
            ))
        }
    }

    /// Returns whether parsed SQL interpolation expressions match placeholders.
    ///
    /// Inputs:
    /// - `self`: analyzed SQL-form metadata.
    /// - `parsed_expression_count`: number of interpolation expression children
    ///   preserved by syntax output.
    ///
    /// Output:
    /// - A stable diagnostic fragment describing whether parser and binding
    ///   parameter counts agree.
    ///
    /// Transformation:
    /// - Compares syntax-output child count with the generated Postgres
    ///   placeholder count so parser/binder drift is visible before wrapper
    ///   lowering.
    pub(crate) fn parameter_count_consistency_message(
        &self,
        parsed_expression_count: usize,
    ) -> String {
        if parsed_expression_count == self.binding.parameter_count {
            "SQL parameter count consistency satisfied".to_string()
        } else {
            format!(
                "SQL parameter count mismatch: parsed {} expression(s), bound {} placeholder(s)",
                parsed_expression_count, self.binding.parameter_count
            )
        }
    }

    /// Returns a diagnostic when SQL result cardinality cannot be inferred.
    ///
    /// Inputs:
    /// - `self`: analyzed SQL-form metadata.
    ///
    /// Output:
    /// - `None` when cardinality is clear; otherwise a stable ambiguity
    ///   diagnostic fragment.
    ///
    /// Transformation:
    /// - Converts conservative cardinality inference into the 0.0.5 rule that
    ///   unclear forms must be rejected until an explicit cardinality syntax is
    ///   added.
    pub(crate) fn cardinality_requirement_message(&self) -> Option<String> {
        if self.cardinality == SqlCardinality::Ambiguous {
            Some(
                "SQL form cardinality is ambiguous; use a clear SELECT, SELECT ... LIMIT 1, or RETURNING shape"
                    .to_string(),
            )
        } else {
            None
        }
    }
}

impl SqlWrapperPlanError {
    /// Returns a stable diagnostic message for this wrapper-planning error.
    ///
    /// Inputs:
    /// - `self`: wrapper plan error variant.
    ///
    /// Output:
    /// - Human-readable diagnostic text.
    ///
    /// Transformation:
    /// - Maps structured wrapper-planning failures into text usable by future
    ///   wrapper lowering diagnostics and tests.
    pub(crate) fn message(&self) -> String {
        match self {
            Self::Analysis(error) => error.message().to_string(),
            Self::NotReady(blockers) => {
                format!("SQL wrapper plan is not ready: {}", blockers.join("; "))
            }
            Self::MissingResultType => "SQL wrapper plan is missing result type".to_string(),
            Self::MissingRowType => "SQL wrapper plan is missing row type".to_string(),
        }
    }
}

/// Error produced while deriving SQL parameter binding metadata.
///
/// Inputs:
/// - Raw SQL text from a compiler-known SQL form.
///
/// Output:
/// - Stable binding error for malformed interpolation syntax.
///
/// Transformation:
/// - Separates binding-shape failures from SQL validation failures so later
///   diagnostics can point at interpolation syntax before any database work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SqlParameterBindingError {
    EmptyInterpolation,
    UnterminatedInterpolation,
}

/// Error produced while building SQL-form analysis metadata.
///
/// Inputs:
/// - Syntax-output raw macro expression metadata.
///
/// Output:
/// - Stable analysis error for malformed or incomplete SQL-form metadata.
///
/// Transformation:
/// - Separates raw syntax-output shape failures from SQL validation failures so
///   future wrapper lowering can reject incomplete compiler metadata early.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SqlFormAnalysisError {
    EmptySql,
    MissingRawSql,
    Binding(SqlParameterBindingError),
}

impl SqlParameterBindingError {
    /// Returns a stable diagnostic message for this binding error.
    ///
    /// Inputs:
    /// - `self`: SQL parameter-binding error variant.
    ///
    /// Output:
    /// - Human-readable diagnostic text.
    ///
    /// Transformation:
    /// - Maps internal error variants to stable messages shared by tests and
    ///   future typechecker diagnostics.
    pub(crate) fn message(&self) -> &'static str {
        match self {
            Self::EmptyInterpolation => "empty SQL interpolation expression",
            Self::UnterminatedInterpolation => "unterminated SQL interpolation expression",
        }
    }
}

impl SqlFormAnalysisError {
    /// Returns a stable diagnostic message for this SQL-form analysis error.
    ///
    /// Inputs:
    /// - `self`: SQL-form analysis error variant.
    ///
    /// Output:
    /// - Human-readable diagnostic text.
    ///
    /// Transformation:
    /// - Converts internal analysis failures into stable messages for tests and
    ///   later typechecker diagnostics.
    pub(crate) fn message(&self) -> &'static str {
        match self {
            Self::EmptySql => "SQL form text must not be empty",
            Self::MissingRawSql => "SQL form is missing raw SQL text",
            Self::Binding(error) => error.message(),
        }
    }
}

impl SqlCardinality {
    /// Returns a stable diagnostic label for this SQL cardinality.
    ///
    /// Inputs:
    /// - `self`: inferred cardinality variant.
    ///
    /// Output:
    /// - A snake_case label suitable for diagnostics and contract tests.
    ///
    /// Transformation:
    /// - Maps internal enum variants to stable text without exposing Rust enum
    ///   casing in user-facing diagnostics.
    pub(crate) fn as_diagnostic_label(self) -> &'static str {
        match self {
            Self::OptionalOne => "optional_one",
            Self::ManyRows => "many_rows",
            Self::AffectedRows => "affected_rows",
            Self::Ambiguous => "ambiguous",
        }
    }

    /// Returns the Terlan wrapper result type implied by this SQL cardinality.
    ///
    /// Inputs:
    /// - `self`: inferred SQL cardinality.
    /// - `row_type`: explicit SQL row type text, when present.
    ///
    /// Output:
    /// - Terlan result type text for clear cardinalities, or `None` when the
    ///   cardinality is ambiguous.
    ///
    /// Transformation:
    /// - Maps source-shape cardinality into the public SQL-form wrapper
    ///   convention: optional single row, row list, affected-row count, or no
    ///   wrapper type for ambiguous forms.
    pub(crate) fn result_type_text(self, row_type: Option<&str>) -> Option<String> {
        let row_type = row_type.unwrap_or("Dynamic");
        match self {
            Self::OptionalOne => Some(format!("Result[Option[{}], Error]", row_type)),
            Self::ManyRows => Some(format!("Result[List[{}], Error]", row_type)),
            Self::AffectedRows => Some("Result[Int, Error]".to_string()),
            Self::Ambiguous => None,
        }
    }
}

/// Infers conservative result cardinality for a compiler-known SQL form.
///
/// Inputs:
/// - `raw`: SQL text preserved from `sql[Row] { ... }`.
///
/// Output:
/// - A conservative `SqlCardinality` value.
///
/// Transformation:
/// - Removes SQL comments and string bodies, tokenizes word-like SQL terms, and
///   recognizes only clear first-statement shapes:
///   `SELECT`, mutating statements with `RETURNING`, mutating statements without
///   `RETURNING`, and `LIMIT 1`. Anything else stays ambiguous for later
///   Postgres-backed validation.
pub(crate) fn infer_sql_cardinality(raw: &str) -> SqlCardinality {
    let tokens = sql_words_without_literals_or_comments(raw);
    let Some(first) = tokens.first().map(String::as_str) else {
        return SqlCardinality::Ambiguous;
    };

    let has_returning = tokens.iter().any(|token| token == "returning");
    let has_limit_one = has_limit_one(&tokens);

    match first {
        "select" => {
            if has_limit_one {
                SqlCardinality::OptionalOne
            } else {
                SqlCardinality::ManyRows
            }
        }
        "insert" | "update" | "delete" => {
            if has_returning {
                if has_limit_one {
                    SqlCardinality::OptionalOne
                } else {
                    SqlCardinality::ManyRows
                }
            } else {
                SqlCardinality::AffectedRows
            }
        }
        _ => SqlCardinality::Ambiguous,
    }
}

/// Builds compiler-facing SQL-form analysis for a syntax-output expression.
///
/// Inputs:
/// - `expr`: syntax-output expression produced by the parser.
///
/// Output:
/// - `Ok(Some(SqlFormAnalysis))` for raw macro expressions named `sql`,
///   `Ok(None)` for all other expressions, or a stable analysis error when the
///   SQL form is structurally incomplete.
///
/// Transformation:
/// - Reads the explicit row type argument metadata, rewrites interpolation
///   islands into Postgres placeholders, and infers conservative cardinality
///   from the preserved SQL text.
pub(crate) fn analyze_sql_form(
    expr: &SyntaxExprOutput,
) -> Result<Option<SqlFormAnalysis>, SqlFormAnalysisError> {
    if expr.kind != SyntaxExprKind::RawMacro || expr.text.as_deref() != Some("sql") {
        return Ok(None);
    }

    let raw = expr
        .raw
        .as_deref()
        .ok_or(SqlFormAnalysisError::MissingRawSql)?;
    if raw.trim().is_empty() {
        return Err(SqlFormAnalysisError::EmptySql);
    }
    let binding = bind_sql_parameters(raw).map_err(SqlFormAnalysisError::Binding)?;
    let cardinality = infer_sql_cardinality(raw);
    let row_type = expr.type_args.first().map(|type_arg| type_arg.text.clone());
    let result_type = cardinality.result_type_text(row_type.as_deref());

    Ok(Some(SqlFormAnalysis {
        row_type,
        row_type_arg_count: expr.type_args.len(),
        binding,
        cardinality,
        result_type,
    }))
}

/// Builds a backend-neutral wrapper-generation plan for a SQL form.
///
/// Inputs:
/// - `expr`: syntax-output expression produced by the parser.
/// - `parsed_expression_count`: number of interpolation expression children
///   preserved by syntax output.
///
/// Output:
/// - `Ok(Some(SqlWrapperPlan))` for ready SQL forms.
/// - `Ok(None)` for non-SQL expressions.
/// - `Err` when SQL metadata is malformed or wrapper prerequisites fail.
///
/// Transformation:
/// - Reuses SQL analysis, applies the readiness gate, and copies only the
///   stable payload needed by future generated wrapper emission.
pub(crate) fn build_sql_wrapper_plan(
    expr: &SyntaxExprOutput,
    parsed_expression_count: usize,
) -> Result<Option<SqlWrapperPlan>, SqlWrapperPlanError> {
    let Some(analysis) = analyze_sql_form(expr).map_err(SqlWrapperPlanError::Analysis)? else {
        return Ok(None);
    };
    let blockers = analysis.wrapper_lowering_blockers(parsed_expression_count);
    if !blockers.is_empty() {
        return Err(SqlWrapperPlanError::NotReady(blockers));
    }

    let row_type = analysis
        .row_type
        .clone()
        .ok_or(SqlWrapperPlanError::MissingRowType)?;
    let result_type = analysis
        .result_type
        .clone()
        .ok_or(SqlWrapperPlanError::MissingResultType)?;
    let projection_fields = expr.raw.as_deref().and_then(simple_sql_projection_fields);

    Ok(Some(SqlWrapperPlan {
        row_type,
        bound_sql: analysis.binding.sql,
        parameter_count: analysis.binding.parameter_count,
        cardinality: analysis.cardinality,
        result_type,
        projection_fields,
    }))
}

/// Rewrites Terlan SQL interpolations into Postgres parameter placeholders.
///
/// Inputs:
/// - `raw`: SQL text preserved from `sql[Row] { ... }`.
///
/// Output:
/// - `SqlParameterBinding` containing rewritten SQL and generated parameter
///   count, or a stable binding error.
///
/// Transformation:
/// - Scans the SQL once, copies quoted SQL strings, quoted identifiers, line
///   comments, and block comments unchanged, and replaces each unquoted
///   `${expr}` island with `$N` in source order.
pub(crate) fn bind_sql_parameters(
    raw: &str,
) -> Result<SqlParameterBinding, SqlParameterBindingError> {
    let chars = raw.chars().collect::<Vec<_>>();
    let mut sql = String::new();
    let mut parameter_count = 0usize;
    let mut index = 0usize;

    while index < chars.len() {
        let current = chars[index];
        let next = chars.get(index + 1).copied();

        if current == '-' && next == Some('-') {
            index = copy_sql_line_comment(&chars, index, &mut sql);
            continue;
        }

        if current == '/' && next == Some('*') {
            index = copy_sql_block_comment(&chars, index, &mut sql);
            continue;
        }

        if current == '\'' || current == '"' {
            index = copy_sql_quoted_segment(&chars, index, current, &mut sql);
            continue;
        }

        if current == '$' && next == Some('{') {
            let (source, next_index) = read_sql_interpolation_source(&chars, index + 2)?;
            if source.trim().is_empty() {
                return Err(SqlParameterBindingError::EmptyInterpolation);
            }
            parameter_count += 1;
            sql.push('$');
            sql.push_str(&parameter_count.to_string());
            index = next_index;
            continue;
        }

        sql.push(current);
        index += 1;
    }

    Ok(SqlParameterBinding {
        sql,
        parameter_count,
    })
}

/// Returns whether normalized SQL tokens include `LIMIT 1`.
///
/// Inputs:
/// - `tokens`: lowercase SQL word/number tokens.
///
/// Output:
/// - `true` when a `limit` token is immediately followed by `1`.
///
/// Transformation:
/// - Scans adjacent token windows instead of parsing SQL so the check stays a
///   conservative source-shape hint.
fn has_limit_one(tokens: &[String]) -> bool {
    tokens
        .windows(2)
        .any(|window| window[0] == "limit" && window[1] == "1")
}
