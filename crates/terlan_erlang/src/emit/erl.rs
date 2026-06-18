use std::collections::{BTreeMap, BTreeSet};

use super::{erlang_type_param_name, is_generic_type_var, map_module_name, map_struct_name};

/// Erlang module render model.
///
/// Inputs:
/// - Lowered module name, documentation lines, and ordered forms.
///
/// Output:
/// - Complete `.erl` source through `render`.
///
/// Transformation:
/// - Keeps top-level form ordering explicit so frontend lowerers control
///   deterministic Erlang output.
#[derive(Debug, Clone)]
pub(super) struct ErlModule {
    pub(super) name: String,
    pub(super) docs: Vec<String>,
    pub(super) forms: Vec<ErlForm>,
}

impl ErlModule {
    /// Renders a complete Erlang module.
    ///
    /// Input is the module name, module docs, and ordered Erlang forms. Output
    /// is `.erl` source text with a module header. The transformation lowers
    /// Terlan module docs into native Erlang `-moduledoc` and preserves form
    /// order so callers control deterministic output.
    pub(super) fn render(&self) -> String {
        let mut out = format!("-module({}).\n\n", self.name);
        out.push_str(&render_doc_attribute("moduledoc", &self.docs));
        for form in &self.forms {
            out.push_str(&form.render());
        }
        out
    }
}

/// Top-level Erlang form render model.
///
/// Inputs:
/// - Lowered exports, types, records, specs, functions, or backend-owned raw
///   text.
///
/// Output:
/// - One top-level Erlang source form through `render`.
///
/// Transformation:
/// - Represents structured forms directly and confines raw fragments to
///   backend-owned escape hatches.
#[derive(Debug, Clone)]
pub(super) enum ErlForm {
    Export(Vec<String>),
    ExportType(Vec<String>),
    Type(ErlTypeDecl),
    Record(ErlRecordDecl),
    Spec(ErlSpec),
    Function(ErlFunction),
    Raw(String),
}

impl ErlForm {
    /// Renders a top-level Erlang form.
    ///
    /// Input is one internal form variant. Output is source text for exactly
    /// that form. The transformation delegates structured forms to their
    /// renderers and passes raw backend-owned text through unchanged.
    pub(super) fn render(&self) -> String {
        match self {
            ErlForm::Export(items) => format!("-export([{}]).\n\n", items.join(", ")),
            ErlForm::ExportType(items) => format!("-export_type([{}]).\n\n", items.join(", ")),
            ErlForm::Type(decl) => decl.render(),
            ErlForm::Record(decl) => decl.render(),
            ErlForm::Spec(spec) => spec.render(),
            ErlForm::Function(function) => function.render(),
            ErlForm::Raw(text) => text.clone(),
        }
    }
}

/// Erlang type declaration render model.
///
/// Inputs:
/// - Opacity marker, docs, type name, parameters, and lowered RHS type.
///
/// Output:
/// - `-type` or `-opaque` Erlang form.
///
/// Transformation:
/// - Preserves declaration shape while marking phantom parameters during
///   rendering.
#[derive(Debug, Clone)]
pub(super) struct ErlTypeDecl {
    pub(super) opaque: bool,
    pub(super) docs: Vec<String>,
    pub(super) name: String,
    pub(super) params: Vec<String>,
    pub(super) rhs: ErlType,
}

impl ErlTypeDecl {
    /// Renders an Erlang type or opaque type declaration.
    ///
    /// Input is a lowered type declaration with docs, params, and right-hand
    /// side. Output is an Erlang `-type` or `-opaque` form. The transformation
    /// marks phantom type parameters with `_` so generated specs remain valid.
    pub(super) fn render(&self) -> String {
        let mut out = render_doc_attribute("doc", &self.docs);
        let mut rhs_vars = BTreeMap::new();
        self.rhs.collect_type_vars(&mut rhs_vars);
        let params = if self.params.is_empty() {
            "()".to_string()
        } else {
            let params = self
                .params
                .iter()
                .map(|param| {
                    let param = erlang_type_param_name(param);
                    if rhs_vars.contains_key(&param) {
                        param
                    } else {
                        format!("_{}", param)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("({})", params)
        };
        let form = if self.opaque { "-opaque" } else { "-type" };
        out.push_str(&format!(
            "{} {}{} :: {}.\n\n",
            form,
            self.name,
            params,
            self.rhs.render()
        ));
        out
    }
}

/// Erlang record declaration render model.
///
/// Inputs:
/// - Backend-safe record name and ordered fields.
///
/// Output:
/// - `-record` Erlang form.
///
/// Transformation:
/// - Keeps field order stable for deterministic generated record headers.
#[derive(Debug, Clone)]
pub(super) struct ErlRecordDecl {
    pub(super) name: String,
    pub(super) fields: Vec<ErlRecordField>,
}

impl ErlRecordDecl {
    /// Renders an Erlang record declaration.
    ///
    /// Input is a record name and ordered fields. Output is a `-record` form.
    /// The transformation preserves field order, renders an empty record body
    /// when no fields are present, and intentionally leaves source docs out of
    /// native Erlang `-doc` attributes because records do not consume them.
    pub(super) fn render(&self) -> String {
        if self.fields.is_empty() {
            return format!("-record({}, {{}}).\n\n", self.name);
        }

        let fields = self
            .fields
            .iter()
            .map(ErlRecordField::render)
            .collect::<Vec<_>>()
            .join(", ");
        format!("-record({}, {{{}}}).\n\n", self.name, fields)
    }
}

/// Erlang record field render model.
///
/// Inputs:
/// - Field name, source docs, and optional default expression.
///
/// Output:
/// - Field fragment inside a `-record` form.
///
/// Transformation:
/// - Preserves default values as expressions and renders docs as inline
///   comments where present.
#[derive(Debug, Clone)]
pub(super) struct ErlRecordField {
    pub(super) name: String,
    pub(super) docs: Vec<String>,
    pub(super) default: Option<ErlExpr>,
}

impl ErlRecordField {
    /// Renders an Erlang record field.
    ///
    /// Input is a field name, optional default expression, and docs. Output is
    /// record-field source text. The transformation appends field docs as an
    /// inline comment because Erlang record field docs have no separate form.
    pub(super) fn render(&self) -> String {
        let field = match &self.default {
            Some(default) => format!("{} = {}", self.name, default.render()),
            None => self.name.clone(),
        };
        if self.docs.is_empty() {
            field
        } else {
            format!("{} % {}", field, self.docs.join(" "))
        }
    }
}

/// Erlang function spec render model.
///
/// Inputs:
/// - Docs, function name, argument types, and return type.
///
/// Output:
/// - `-spec` Erlang form.
///
/// Transformation:
/// - Tracks type-variable usage so render can mark phantom variables.
#[derive(Debug, Clone)]
pub(super) struct ErlSpec {
    pub(super) docs: Vec<String>,
    pub(super) name: String,
    pub(super) args: Vec<ErlType>,
    pub(super) ret: ErlType,
}

impl ErlSpec {
    /// Renders an Erlang function spec.
    ///
    /// Input is a function name, argument types, return type, and docs. Output
    /// is a `-spec` form. The transformation detects type variables that occur
    /// only once and renders them as phantom variables for Dialyzer-friendly
    /// specs.
    pub(super) fn render(&self) -> String {
        let mut out = render_doc_attribute("doc", &self.docs);
        let mut vars = BTreeMap::new();
        for arg in &self.args {
            arg.collect_type_vars(&mut vars);
        }
        self.ret.collect_type_vars(&mut vars);
        let phantom_vars = vars
            .into_iter()
            .filter_map(|(name, count)| (count == 1).then_some(name))
            .collect::<BTreeSet<_>>();
        let args = self
            .args
            .iter()
            .map(|arg| arg.render_with_phantom_vars(&phantom_vars))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "-spec {}({}) -> {}.\n\n",
            self.name,
            args,
            self.ret.render_with_phantom_vars(&phantom_vars)
        ));
        out
    }
}

/// Erlang function render model.
///
/// Inputs:
/// - Function docs, backend-safe name, and ordered clauses.
///
/// Output:
/// - Erlang function source.
///
/// Transformation:
/// - Shares one function name across all clauses and renders clause
///   separators based on position.
#[derive(Debug, Clone)]
pub(super) struct ErlFunction {
    pub(super) docs: Vec<String>,
    pub(super) name: String,
    pub(super) clauses: Vec<ErlFunctionClause>,
}

impl ErlFunction {
    /// Renders an Erlang function.
    ///
    /// Input is function docs, name, and ordered clauses. Output is Erlang
    /// function source. The transformation gives every clause the shared name
    /// and selects `;` or `.` terminators based on clause position.
    pub(super) fn render(&self) -> String {
        let mut out = render_doc_attribute("doc", &self.docs);
        for (index, clause) in self.clauses.iter().enumerate() {
            let is_last = index + 1 == self.clauses.len();
            out.push_str(&clause.render(&self.name, is_last));
            out.push('\n');
        }
        out
    }
}

/// Renders Terlan documentation lines as an Erlang documentation attribute.
///
/// Input is a sequence of already extracted doc strings. Output is a possibly
/// empty native documentation attribute. The transformation joins source doc
/// lines with newlines and emits `-moduledoc` or `-doc` so BEAM builds can
/// carry EEP-48/HexDocs-readable documentation instead of loose comments.
fn render_doc_attribute(attribute: &str, docs: &[String]) -> String {
    if docs.is_empty() {
        return String::new();
    }

    format!(
        "-{} \"{}\".\n\n",
        attribute,
        escape_erlang_string_literal(&docs.join("\n"))
    )
}

/// Escapes text for use in an Erlang string literal.
///
/// Input is raw documentation text. Output is text safe to place between
/// Erlang double quotes. The transformation preserves newlines as `\n` escapes
/// and escapes backslashes, quotes, carriage returns, and tabs.
fn escape_erlang_string_literal(text: &str) -> String {
    text.chars()
        .flat_map(|ch| match ch {
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            other => vec![other],
        })
        .collect()
}

/// Erlang function clause render model.
///
/// Inputs:
/// - Clause patterns, optional guard, and body expression.
///
/// Output:
/// - One named Erlang function clause through `render`.
///
/// Transformation:
/// - Leaves the owning function name outside the clause so multi-clause
///   functions can reuse the same clause representation.
#[derive(Debug, Clone)]
pub(super) struct ErlFunctionClause {
    pub(super) patterns: Vec<ErlPattern>,
    pub(super) guard: Option<ErlExpr>,
    pub(super) body: ErlExpr,
}

impl ErlFunctionClause {
    /// Renders one Erlang function clause.
    ///
    /// Inputs are the owning function name and whether this is the final
    /// clause. Output is source text for one clause. The transformation renders
    /// patterns, optional guards, body expression, and a valid Erlang clause
    /// terminator.
    pub(super) fn render(&self, name: &str, is_last: bool) -> String {
        let args = self
            .patterns
            .iter()
            .map(ErlPattern::render)
            .collect::<Vec<_>>()
            .join(", ");
        let guard = self
            .guard
            .as_ref()
            .map(|guard| format!(" when {}", guard.render()))
            .unwrap_or_default();
        let terminator = if is_last { "." } else { ";" };
        format!(
            "{}({}){} ->\n    {}{}\n",
            name,
            args,
            guard,
            self.body.render(),
            terminator
        )
    }
}

/// Erlang type expression render model.
///
/// Inputs:
/// - Lowered type fragments from syntax or CoreIR type lowering.
///
/// Output:
/// - Erlang type syntax through `render`.
///
/// Transformation:
/// - Represents common type shapes structurally while allowing raw fragments
///   for backend-owned spec forms.
#[derive(Debug, Clone)]
pub(super) enum ErlType {
    Raw(String),
    Named {
        name: String,
        args: Vec<ErlType>,
    },
    Tuple(Vec<ErlType>),
    List(Box<ErlType>),
    Map(Vec<ErlMapTypeField>),
    Union(Vec<ErlType>),
    Fun {
        args: Vec<ErlType>,
        ret: Box<ErlType>,
    },
}

impl ErlType {
    /// Renders an Erlang type expression.
    ///
    /// Input is an internal type expression. Output is Erlang type syntax. The
    /// transformation recursively renders named, tuple, list, union, and
    /// function types while preserving raw backend fragments.
    pub(super) fn render(&self) -> String {
        match self {
            ErlType::Raw(text) => text.clone(),
            ErlType::Named { name, args } if args.is_empty() => format!("{}()", name),
            ErlType::Named { name, args } => {
                let args = args
                    .iter()
                    .map(ErlType::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}({})", name, args)
            }
            ErlType::Tuple(items) => {
                let items = items
                    .iter()
                    .map(ErlType::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{}}}", items)
            }
            ErlType::List(inner) => format!("[{}]", inner.render()),
            ErlType::Map(fields) => {
                let fields = fields
                    .iter()
                    .map(ErlMapTypeField::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("#{{{}}}", fields)
            }
            ErlType::Union(items) => items
                .iter()
                .map(ErlType::render)
                .collect::<Vec<_>>()
                .join(" | "),
            ErlType::Fun { args, ret } if args.is_empty() => format!("fun(() -> {})", ret.render()),
            ErlType::Fun { args, ret } => {
                let args = args
                    .iter()
                    .map(ErlType::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("fun(({}) -> {})", args, ret.render())
            }
        }
    }

    /// Normalizes degenerate Erlang type expressions.
    ///
    /// Input is an owned type expression. Output is the same expression with
    /// single-item unions collapsed. The transformation keeps all other type
    /// shapes unchanged.
    pub(super) fn normalized(self) -> Self {
        match self {
            ErlType::Union(items) if items.len() == 1 => items.into_iter().next().unwrap(),
            other => other,
        }
    }

    /// Counts generic type variable occurrences.
    ///
    /// Inputs are a type expression and mutable occurrence map. Output is the
    /// mutated map. The transformation walks nested type shapes and increments
    /// counts only for raw names classified as generic type variables.
    fn collect_type_vars(&self, vars: &mut BTreeMap<String, usize>) {
        match self {
            ErlType::Raw(text) if is_generic_type_var(text) => {
                *vars.entry(text.clone()).or_insert(0) += 1;
            }
            ErlType::Raw(_) => {}
            ErlType::Named { args, .. } | ErlType::Tuple(args) | ErlType::Union(args) => {
                for arg in args {
                    arg.collect_type_vars(vars);
                }
            }
            ErlType::List(inner) => inner.collect_type_vars(vars),
            ErlType::Map(fields) => {
                for field in fields {
                    field.value.collect_type_vars(vars);
                }
            }
            ErlType::Fun { args, ret } => {
                for arg in args {
                    arg.collect_type_vars(vars);
                }
                ret.collect_type_vars(vars);
            }
        }
    }

    /// Renders an Erlang type expression with phantom variables marked.
    ///
    /// Inputs are a type expression and the set of variable names considered
    /// phantom. Output is Erlang type syntax. The transformation recursively
    /// renders the type while prefixing phantom raw variables with `_`.
    fn render_with_phantom_vars(&self, phantom_vars: &BTreeSet<String>) -> String {
        match self {
            ErlType::Raw(text) if phantom_vars.contains(text) => format!("_{}", text),
            ErlType::Raw(text) => text.clone(),
            ErlType::Named { name, args } if args.is_empty() => format!("{}()", name),
            ErlType::Named { name, args } => {
                let args = args
                    .iter()
                    .map(|arg| arg.render_with_phantom_vars(phantom_vars))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}({})", name, args)
            }
            ErlType::Tuple(items) => {
                let items = items
                    .iter()
                    .map(|item| item.render_with_phantom_vars(phantom_vars))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{}}}", items)
            }
            ErlType::List(inner) => format!("[{}]", inner.render_with_phantom_vars(phantom_vars)),
            ErlType::Map(fields) => {
                let fields = fields
                    .iter()
                    .map(|field| field.render_with_phantom_vars(phantom_vars))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("#{{{}}}", fields)
            }
            ErlType::Union(items) => items
                .iter()
                .map(|item| item.render_with_phantom_vars(phantom_vars))
                .collect::<Vec<_>>()
                .join(" | "),
            ErlType::Fun { args, ret } if args.is_empty() => {
                format!("fun(() -> {})", ret.render_with_phantom_vars(phantom_vars))
            }
            ErlType::Fun { args, ret } => {
                let args = args
                    .iter()
                    .map(|arg| arg.render_with_phantom_vars(phantom_vars))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "fun(({}) -> {})",
                    args,
                    ret.render_with_phantom_vars(phantom_vars)
                )
            }
        }
    }
}

/// Erlang map type field render model.
///
/// Inputs:
/// - Key text, value type, and requiredness marker.
///
/// Output:
/// - Map type field syntax.
///
/// Transformation:
/// - Selects Erlang `:=` or `=>` separators from the requiredness flag.
#[derive(Debug, Clone)]
pub(super) struct ErlMapTypeField {
    pub(super) key: String,
    pub(super) value: ErlType,
    pub(super) required: bool,
}

impl ErlMapTypeField {
    /// Renders an Erlang map type field.
    ///
    /// Input is a key, lowered value type, and requiredness flag. Output is
    /// Erlang map type syntax. The transformation selects `:=` for required
    /// keys and `=>` for optional/associative keys.
    fn render(&self) -> String {
        let sep = if self.required { ":=" } else { "=>" };
        format!("{}{}{}", self.key, sep, self.value.render())
    }

    /// Renders an Erlang map type field with phantom type variables marked.
    ///
    /// Inputs are one map type field and the phantom-variable set collected
    /// from the enclosing spec. Output is Erlang map type syntax. The
    /// transformation delegates value rendering to the nested type mapper.
    fn render_with_phantom_vars(&self, phantom_vars: &BTreeSet<String>) -> String {
        let sep = if self.required { ":=" } else { "=>" };
        format!(
            "{}{}{}",
            self.key,
            sep,
            self.value.render_with_phantom_vars(phantom_vars)
        )
    }
}

/// Erlang expression render model.
///
/// Inputs:
/// - Lowered expression forms from CoreIR or syntax-output lowering.
///
/// Output:
/// - Erlang expression source through `render`.
///
/// Transformation:
/// - Captures the common expression subset needed by Terlan's BEAM target
///   while keeping raw fragments isolated to backend-owned cases.
#[derive(Debug, Clone)]
pub(super) enum ErlExpr {
    Int(i64),
    Float(String),
    Atom(String),
    Var(String),
    Binary(String),
    Tuple(Vec<ErlExpr>),
    List(Vec<ErlExpr>),
    FixedArray(Vec<ErlExpr>),
    ListCons(Box<ErlExpr>, Box<ErlExpr>),
    Map(Vec<ErlMapField>),
    RecordConstruct {
        name: String,
        fields: Vec<ErlMapField>,
    },
    RecordAccess {
        value: Box<ErlExpr>,
        name: String,
        field: String,
    },
    RecordUpdate {
        value: Box<ErlExpr>,
        name: String,
        fields: Vec<ErlMapField>,
    },
    ListComprehension {
        expr: Box<ErlExpr>,
        pattern: ErlPattern,
        source: Box<ErlExpr>,
        guard: Option<Box<ErlExpr>>,
    },
    Let {
        bindings: Vec<ErlLetBinding>,
        body: Box<ErlExpr>,
    },
    Call {
        module: Option<String>,
        function: String,
        args: Vec<ErlExpr>,
    },
    Apply {
        callee: Box<ErlExpr>,
        args: Vec<ErlExpr>,
    },
    Case {
        scrutinee: Box<ErlExpr>,
        clauses: Vec<ErlCaseClause>,
    },
    Try {
        body: Box<ErlExpr>,
        of_clauses: Vec<ErlCaseClause>,
        catch_clauses: Vec<ErlCaseClause>,
        after_clause: Option<ErlTryAfterClause>,
    },
    If(Vec<ErlIfClause>),
    Fun(Vec<ErlFunctionClause>),
    RemoteFunRef {
        module: String,
        function: String,
        arity: usize,
    },
    MacroCall {
        name: String,
        args: Vec<ErlExpr>,
    },
    BinaryOp {
        op: ErlBinaryOp,
        left: Box<ErlExpr>,
        right: Box<ErlExpr>,
    },
    UnaryOp {
        op: ErlUnaryOp,
        expr: Box<ErlExpr>,
    },
    Index {
        value: Box<ErlExpr>,
        index: Box<ErlExpr>,
    },
    Raw(String),
}

impl ErlExpr {
    /// Renders an Erlang expression.
    ///
    /// Input is an internal expression tree. Output is Erlang expression
    /// source. The transformation recursively renders supported expression
    /// forms, applying BEAM naming hygiene for modules, records, and atoms.
    pub(super) fn render(&self) -> String {
        match self {
            ErlExpr::Int(value) => value.to_string(),
            ErlExpr::Float(value) => value.clone(),
            ErlExpr::Atom(name) => render_atom_expr(name),
            ErlExpr::Var(name) => name.clone(),
            ErlExpr::Binary(value) => value.clone(),
            ErlExpr::Tuple(items) => {
                let items = items
                    .iter()
                    .map(ErlExpr::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{}}}", items)
            }
            ErlExpr::List(items) => {
                let items = items
                    .iter()
                    .map(ErlExpr::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("[{}]", items)
            }
            ErlExpr::FixedArray(items) => {
                let items = items
                    .iter()
                    .map(ErlExpr::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{}}}", items)
            }
            ErlExpr::ListCons(left, right) => format!("[{}|{}]", left.render(), right.render()),
            ErlExpr::Map(fields) => {
                let fields = fields
                    .iter()
                    .map(ErlMapField::render_map)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("#{{{}}}", fields)
            }
            ErlExpr::RecordConstruct { name, fields } => render_record(name, fields),
            ErlExpr::RecordAccess { value, name, field } => {
                format!("{}#{}.{}", value.render(), map_struct_name(name), field)
            }
            ErlExpr::RecordUpdate {
                value,
                name,
                fields,
            } => {
                format!("{}{}", value.render(), render_record(name, fields))
            }
            ErlExpr::ListComprehension {
                expr,
                pattern,
                source,
                guard,
            } => {
                let guard = guard
                    .as_ref()
                    .map(|guard| format!(", {}", guard.render()))
                    .unwrap_or_default();
                format!(
                    "[{} || {} <- {}{}]",
                    expr.render(),
                    pattern.render(),
                    source.render(),
                    guard
                )
            }
            ErlExpr::Let { bindings, body } => {
                let mut out = String::from("begin\n");
                for binding in bindings {
                    out.push_str(&format!(
                        "    {} = {},\n",
                        binding.name,
                        binding.value.render()
                    ));
                }
                out.push_str(&format!("    {}\nend", body.render()));
                out
            }
            ErlExpr::Call {
                module,
                function,
                args,
            } => {
                let args = args
                    .iter()
                    .map(ErlExpr::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                match module {
                    Some(module) => {
                        format!("{}:{}({})", map_module_name(module), function, args)
                    }
                    None => format!("{}({})", function, args),
                }
            }
            ErlExpr::Apply { callee, args } => {
                let args = args
                    .iter()
                    .map(ErlExpr::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("({})({})", callee.render(), args)
            }
            ErlExpr::Case { scrutinee, clauses } => {
                let mut out = format!("case {} of\n", scrutinee.render());
                for (idx, clause) in clauses.iter().enumerate() {
                    let suffix = if idx + 1 == clauses.len() { "" } else { ";" };
                    out.push_str(&format!("    {}{}\n", clause.render(), suffix));
                }
                out.push_str("end");
                out
            }
            ErlExpr::Try {
                body,
                of_clauses,
                catch_clauses,
                after_clause,
            } => {
                let mut out = format!("try {}", body.render());
                if !of_clauses.is_empty() {
                    out.push_str("\nof\n");
                    for (idx, clause) in of_clauses.iter().enumerate() {
                        let suffix = if idx + 1 == of_clauses.len() { "" } else { ";" };
                        out.push_str(&format!("    {}{}\n", clause.render(), suffix));
                    }
                }
                if !catch_clauses.is_empty() {
                    out.push_str("\ncatch\n");
                    for (idx, clause) in catch_clauses.iter().enumerate() {
                        let suffix = if idx + 1 == catch_clauses.len() {
                            ""
                        } else {
                            ";"
                        };
                        out.push_str(&format!("    {}{}\n", clause.render(), suffix));
                    }
                }
                if let Some(after) = after_clause {
                    out.push_str("\nafter\n");
                    out.push_str(&format!(
                        "    {} -> {}\n",
                        after.trigger.render(),
                        after.body.render()
                    ));
                }
                out.push_str("end");
                out
            }
            ErlExpr::If(clauses) => {
                let mut out = String::from("if\n");
                for (idx, clause) in clauses.iter().enumerate() {
                    let suffix = if idx + 1 == clauses.len() { "" } else { ";" };
                    out.push_str(&format!(
                        "    {} -> {}{}\n",
                        clause.condition.render(),
                        clause.body.render(),
                        suffix
                    ));
                }
                out.push_str("end");
                out
            }
            ErlExpr::Fun(clauses) => {
                let mut out = String::from("fun\n");
                for (idx, clause) in clauses.iter().enumerate() {
                    let args = clause
                        .patterns
                        .iter()
                        .map(ErlPattern::render)
                        .collect::<Vec<_>>()
                        .join(", ");
                    let suffix = if idx + 1 == clauses.len() { "" } else { ";" };
                    out.push_str(&format!(
                        "    ({}) -> {}{}\n",
                        args,
                        clause.body.render(),
                        suffix
                    ));
                }
                out.push_str("end");
                out
            }
            ErlExpr::RemoteFunRef {
                module,
                function,
                arity,
            } => format!("fun {}:{}/{}", map_module_name(module), function, arity),
            ErlExpr::MacroCall { name, args } if args.is_empty() => format!("?{}", name),
            ErlExpr::MacroCall { name, args } => {
                let args = args
                    .iter()
                    .map(ErlExpr::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("?{}({})", name, args)
            }
            ErlExpr::BinaryOp { op, left, right } => {
                format!("{} {} {}", left.render(), op.render(), right.render())
            }
            ErlExpr::UnaryOp { op, expr } => match op {
                ErlUnaryOp::Neg => format!("-{}", expr.render()),
                ErlUnaryOp::Not => format!("not {}", expr.render()),
            },
            ErlExpr::Index { value, index } => {
                format!("element(({}) + 1, {})", index.render(), value.render())
            }
            ErlExpr::Raw(text) => text.clone(),
        }
    }
}

/// Erlang `begin` binding render model.
///
/// Inputs:
/// - Backend-safe variable name and expression value.
///
/// Output:
/// - Binding fragment inside a rendered `begin ... end` expression.
///
/// Transformation:
/// - Lets `let` lowering keep ordered value bindings distinct from the final
///   body expression.
#[derive(Debug, Clone)]
pub(super) struct ErlLetBinding {
    pub(super) name: String,
    pub(super) value: ErlExpr,
}

/// Erlang map or record field expression render model.
///
/// Inputs:
/// - Field key, value expression, and map requiredness marker.
///
/// Output:
/// - Map field or record field assignment source.
///
/// Transformation:
/// - Shares one lowered field shape across map expressions, record
///   construction, and record updates.
#[derive(Debug, Clone)]
pub(super) struct ErlMapField {
    pub(super) key: String,
    pub(super) value: ErlExpr,
    pub(super) required: bool,
}

impl ErlMapField {
    /// Renders a map field expression.
    ///
    /// Input is a lowered key/value pair plus requiredness. Output is Erlang
    /// map field syntax. The transformation selects `:=` for required updates
    /// and `=>` for associative construction.
    fn render_map(&self) -> String {
        let sep = if self.required { ":=" } else { "=>" };
        format!("{}{}{}", self.key, sep, self.value.render())
    }

    /// Renders a record field assignment.
    ///
    /// Input is a lowered key/value pair. Output is Erlang record field syntax.
    /// The transformation ignores map requiredness because record assignments
    /// always use `=`.
    fn render_record(&self) -> String {
        format!("{} = {}", self.key, self.value.render())
    }
}

/// Erlang case-like clause render model.
///
/// Inputs:
/// - Pattern, optional guard, and body expression.
///
/// Output:
/// - Clause fragment for `case`, `try of`, and `catch`.
///
/// Transformation:
/// - Keeps separator management with the enclosing expression renderer.
#[derive(Debug, Clone)]
pub(super) struct ErlCaseClause {
    pub(super) pattern: ErlPattern,
    pub(super) guard: Option<ErlExpr>,
    pub(super) body: ErlExpr,
}

/// Erlang `try ... after` clause render model.
///
/// Inputs:
/// - Trigger expression and cleanup body expression.
///
/// Output:
/// - `after` clause fragment inside a rendered try expression.
///
/// Transformation:
/// - Models Terlan's try-after cleanup branch without turning it into a normal
///   case clause.
#[derive(Debug, Clone)]
pub(super) struct ErlTryAfterClause {
    pub(super) trigger: Box<ErlExpr>,
    pub(super) body: Box<ErlExpr>,
}

impl ErlCaseClause {
    /// Renders a case-like clause.
    ///
    /// Input is a pattern, optional guard, and body expression. Output is
    /// Erlang clause text without the outer clause separator. The
    /// transformation renders the guard only when present.
    pub(super) fn render(&self) -> String {
        let guard = self
            .guard
            .as_ref()
            .map(|guard| format!(" when {}", guard.render()))
            .unwrap_or_default();
        format!(
            "{}{} -> {}",
            self.pattern.render(),
            guard,
            self.body.render()
        )
    }
}

/// Erlang `if` clause render model.
///
/// Inputs:
/// - Condition expression and body expression.
///
/// Output:
/// - Clause fragment inside an Erlang `if`.
///
/// Transformation:
/// - Separates condition/body pairs from the surrounding clause separator
///   logic.
#[derive(Debug, Clone)]
pub(super) struct ErlIfClause {
    pub(super) condition: ErlExpr,
    pub(super) body: ErlExpr,
}

/// Erlang pattern render model.
///
/// Inputs:
/// - Lowered pattern shapes from syntax or CoreIR pattern lowering.
///
/// Output:
/// - Erlang pattern source through `render`.
///
/// Transformation:
/// - Captures literals, collections, maps, records, and variables in a backend
///   hygiene-aware shape.
#[derive(Debug, Clone)]
pub(super) enum ErlPattern {
    Wildcard,
    Var(String),
    Int(i64),
    Float(String),
    Atom(String),
    Tuple(Vec<ErlPattern>),
    List(Vec<ErlPattern>),
    ListCons(Box<ErlPattern>, Box<ErlPattern>),
    Map(Vec<ErlPatternMapField>),
    Record {
        name: String,
        fields: Vec<ErlPatternMapField>,
    },
}

impl ErlPattern {
    /// Renders an Erlang pattern.
    ///
    /// Input is an internal pattern tree. Output is Erlang pattern source. The
    /// transformation recursively renders tuple, list, cons, map, record, and
    /// literal patterns with backend-safe record and atom spelling.
    pub(super) fn render(&self) -> String {
        match self {
            ErlPattern::Wildcard => "_".to_string(),
            ErlPattern::Var(name) => name.clone(),
            ErlPattern::Int(value) => value.to_string(),
            ErlPattern::Float(value) => value.clone(),
            ErlPattern::Atom(name) => render_atom_expr(name),
            ErlPattern::Tuple(items) => {
                let items = items
                    .iter()
                    .map(ErlPattern::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{}}}", items)
            }
            ErlPattern::List(items) => {
                let items = items
                    .iter()
                    .map(ErlPattern::render)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("[{}]", items)
            }
            ErlPattern::ListCons(left, right) => format!("[{}|{}]", left.render(), right.render()),
            ErlPattern::Map(fields) => {
                let fields = fields
                    .iter()
                    .map(ErlPatternMapField::render_map)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("#{{{}}}", fields)
            }
            ErlPattern::Record { name, fields } => {
                let fields = fields
                    .iter()
                    .map(ErlPatternMapField::render_record)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("#{}{{{}}}", map_struct_name(name), fields)
            }
        }
    }
}

/// Erlang map or record pattern field render model.
///
/// Inputs:
/// - Field key, lowered pattern value, and map requiredness marker.
///
/// Output:
/// - Map pattern field or record pattern field source.
///
/// Transformation:
/// - Shares one field shape between map and record pattern rendering.
#[derive(Debug, Clone)]
pub(super) struct ErlPatternMapField {
    pub(super) key: String,
    pub(super) value: ErlPattern,
    pub(super) required: bool,
}

impl ErlPatternMapField {
    /// Renders a map pattern field.
    ///
    /// Input is a lowered pattern field plus requiredness. Output is Erlang map
    /// pattern syntax. The transformation selects `:=` for required matches and
    /// `=>` when the field is associative.
    fn render_map(&self) -> String {
        let sep = if self.required { ":=" } else { "=>" };
        format!("{}{}{}", self.key, sep, self.value.render())
    }

    /// Renders a record pattern field.
    ///
    /// Input is a lowered pattern field. Output is Erlang record pattern field
    /// syntax. The transformation ignores map requiredness because record
    /// pattern fields always use `=`.
    fn render_record(&self) -> String {
        format!("{} = {}", self.key, self.value.render())
    }
}

/// Erlang binary operator identity.
///
/// Inputs:
/// - Terlan/CoreIR operator lowering decisions.
///
/// Output:
/// - Backend operator spelling through `render`.
///
/// Transformation:
/// - Keeps source/operator normalization separate from expression rendering.
#[derive(Debug, Clone)]
pub(super) enum ErlBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    EqEq,
    EqEqEq,
    NotEq,
    NotEqEq,
    GtEq,
    Lt,
    Gt,
    LtEq,
    DivRem,
    Rem,
    And,
    Or,
    PipeForward,
    Send,
}

/// Erlang unary operator identity.
///
/// Inputs:
/// - Terlan/CoreIR unary operator lowering decisions.
///
/// Output:
/// - Backend unary operator spelling selected during expression rendering.
///
/// Transformation:
/// - Represents negation and logical-not independently from operand lowering.
#[derive(Debug, Clone)]
pub(super) enum ErlUnaryOp {
    Neg,
    Not,
}

impl ErlBinaryOp {
    /// Renders an Erlang binary operator token.
    ///
    /// Input is a lowered operator identity. Output is the Erlang token used by
    /// expression rendering. The transformation maps Terlan/CoreIR logical and
    /// arithmetic identities onto BEAM-compatible operator spellings.
    pub(super) fn render(&self) -> &'static str {
        match self {
            ErlBinaryOp::Add => "+",
            ErlBinaryOp::Sub => "-",
            ErlBinaryOp::Mul => "*",
            ErlBinaryOp::Div => "/",
            ErlBinaryOp::Eq => "==",
            ErlBinaryOp::EqEq => "=:=",
            ErlBinaryOp::EqEqEq => "=:=",
            ErlBinaryOp::NotEq => "/=",
            ErlBinaryOp::NotEqEq => "=/=",
            ErlBinaryOp::GtEq => ">=",
            ErlBinaryOp::Lt => "<",
            ErlBinaryOp::Gt => ">",
            ErlBinaryOp::LtEq => "=<",
            ErlBinaryOp::DivRem => "div",
            ErlBinaryOp::Rem => "rem",
            ErlBinaryOp::And => "andalso",
            ErlBinaryOp::Or => "orelse",
            ErlBinaryOp::PipeForward => "|>",
            ErlBinaryOp::Send => "!",
        }
    }
}
/// Renders an Erlang record expression fragment.
///
/// Inputs are a Terlan record/struct name and lowered fields. Output is
/// `#record{...}` source. The transformation maps the Terlan name through the
/// backend struct-name hygiene rule before joining field assignments.
fn render_record(name: &str, fields: &[ErlMapField]) -> String {
    let fields = fields
        .iter()
        .map(ErlMapField::render_record)
        .collect::<Vec<_>>()
        .join(", ");
    format!("#{}{{{}}}", map_struct_name(name), fields)
}

/// Renders an Erlang atom expression or pattern literal.
///
/// Input is an atom name without source-level punctuation. Output is Erlang
/// atom syntax. The transformation leaves known keyword atoms bare and quotes
/// all other atoms with Erlang atom escaping.
pub(super) fn render_atom_expr(name: &str) -> String {
    if is_atom_keyword(name) {
        name.to_string()
    } else {
        format!("'{}'", escape_quoted_atom(name))
    }
}

/// Checks whether an atom can render without quoting.
///
/// Input is an atom name. Output is true only for the backend's small keyword
/// allowlist. The transformation is a pure membership check used before quoted
/// atom rendering.
fn is_atom_keyword(name: &str) -> bool {
    matches!(name, "true" | "false" | "ok" | "error" | "nil" | "unit")
}

/// Escapes atom text for single-quoted Erlang atom syntax.
///
/// Input is raw atom text. Output is escaped atom text without surrounding
/// quotes. The transformation escapes backslashes and single quotes.
fn escape_quoted_atom(name: &str) -> String {
    name.replace('\\', "\\\\").replace('\'', "\\'")
}
