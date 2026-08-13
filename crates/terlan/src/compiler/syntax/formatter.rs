pub(super) use std::collections::{BTreeMap, BTreeSet};

pub(super) use crate::terlan_syntax::parse_tree::{
    Annotation, BinaryLayoutField, BinaryOp, CaseClause, ConstructorClause, Decl, Expr,
    HtmlAttrValue, HtmlBlockExpr, HtmlNode, ImportDecl, ImportItem, ImportKind, MapExprField,
    MapField, Module, Param, Pattern, StringPatternSegment, TypeExpr, UnaryOp,
};
pub(super) use crate::terlan_syntax::parser::{
    parse_interface_module, parse_module, parse_script_for_format, ParseError,
};
pub(super) use crate::terlan_syntax::syntax_output::binary_op_text;

mod comprehension;
mod declaration_formatting;
mod declarations;
mod expression_formatting;
mod function_references;
mod grouped_cases;
mod html;
mod import_analysis;
mod let_else;
mod literals;
mod metadata;
mod precedence;
mod reference_rewriting;
mod repeated_lets;

use super::quoted_string_literal;
use comprehension::format_list_comprehension;
use declarations::*;
use html::format_html_block;
use let_else::{format_function_body_let, format_let_expr};
use literals::format_float_literal;
use metadata::*;
pub use repeated_lets::{
    format_source_module_migrating_repeated_lets, migrate_repeated_let_source,
};

use declaration_formatting::format_docs;
pub(crate) use declaration_formatting::format_pattern;
use expression_formatting::{
    format_assignment_child, format_case_clause, format_expr, format_let_binding_assignment,
    format_let_binding_value, format_statement_parts, format_type_expr,
};
pub(crate) use import_analysis::format_module;
use import_analysis::DEFAULT_MAX_LINE_LENGTH;
pub use import_analysis::{
    format_interface_source_module, format_script_source, format_source_module,
};

#[cfg(test)]
#[path = "formatter_test.rs"]
#[cfg(test)]
mod formatter_test;

#[cfg(test)]
#[path = "formatter_let_else_test.rs"]
#[cfg(test)]
mod formatter_let_else_test;
