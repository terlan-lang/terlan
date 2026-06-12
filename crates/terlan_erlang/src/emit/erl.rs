use std::collections::{BTreeMap, BTreeSet};

use super::{erlang_type_param_name, is_generic_type_var, map_module_name, map_struct_name};

#[derive(Debug, Clone)]
pub(super) struct ErlModule {
    pub(super) name: String,
    pub(super) forms: Vec<ErlForm>,
}

impl ErlModule {
    /// Renders a complete Erlang module.
    ///
    /// Input is the module name plus ordered Erlang forms. Output is `.erl`
    /// source text with a module header. The transformation preserves form
    /// order so callers control deterministic output.
    pub(super) fn render(&self) -> String {
        let mut out = format!("-module({}).\n\n", self.name);
        for form in &self.forms {
            out.push_str(&form.render());
        }
        out
    }
}

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
        let mut out = render_edoc(&self.docs);
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

#[derive(Debug, Clone)]
pub(super) struct ErlRecordDecl {
    pub(super) docs: Vec<String>,
    pub(super) name: String,
    pub(super) fields: Vec<ErlRecordField>,
}

impl ErlRecordDecl {
    /// Renders an Erlang record declaration.
    ///
    /// Input is a record name, docs, and ordered fields. Output is a `-record`
    /// form. The transformation preserves field order and renders an empty
    /// record body when no fields are present.
    pub(super) fn render(&self) -> String {
        let mut out = render_edoc(&self.docs);
        if self.fields.is_empty() {
            out.push_str(&format!("-record({}, {{}}).\n\n", self.name));
            return out;
        }

        let fields = self
            .fields
            .iter()
            .map(ErlRecordField::render)
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("-record({}, {{{}}}).\n\n", self.name, fields));
        out
    }
}

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
        let mut out = render_edoc(&self.docs);
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
        let mut out = render_edoc(&self.docs);
        for (index, clause) in self.clauses.iter().enumerate() {
            let is_last = index + 1 == self.clauses.len();
            out.push_str(&clause.render(&self.name, is_last));
            out.push('\n');
        }
        out
    }
}

/// Renders Terlan documentation lines as Erlang EDoc comments.
///
/// Input is a sequence of already extracted doc strings. Output is a possibly
/// empty comment block. The transformation prefixes each line with `%% @doc`
/// and separates non-empty doc blocks from the following form.
fn render_edoc(docs: &[String]) -> String {
    let mut out = String::new();
    for line in docs {
        out.push_str("%% @doc ");
        out.push_str(line);
        out.push('\n');
    }
    if !docs.is_empty() {
        out.push('\n');
    }
    out
}

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

#[derive(Debug, Clone)]
pub(super) enum ErlType {
    Raw(String),
    Named {
        name: String,
        args: Vec<ErlType>,
    },
    Tuple(Vec<ErlType>),
    List(Box<ErlType>),
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
    Receive {
        clauses: Vec<ErlCaseClause>,
        after_clause: Option<ErlTryAfterClause>,
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
                    .map(|guard| format!(" when {}", guard.render()))
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
            ErlExpr::Receive {
                clauses,
                after_clause,
            } => {
                let mut out = String::from("receive\n");
                for (idx, clause) in clauses.iter().enumerate() {
                    let suffix = if idx + 1 == clauses.len() { "" } else { ";" };
                    out.push_str(&format!("    {}{}\n", clause.render(), suffix));
                }
                if let Some(after) = after_clause {
                    out.push_str("after\n");
                    out.push_str(&format!(
                        "    {} -> {}\n",
                        after.trigger.render(),
                        after.body.render()
                    ));
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

#[derive(Debug, Clone)]
pub(super) struct ErlLetBinding {
    pub(super) name: String,
    pub(super) value: ErlExpr,
}

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

#[derive(Debug, Clone)]
pub(super) struct ErlCaseClause {
    pub(super) pattern: ErlPattern,
    pub(super) guard: Option<ErlExpr>,
    pub(super) body: ErlExpr,
}

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

#[derive(Debug, Clone)]
pub(super) struct ErlIfClause {
    pub(super) condition: ErlExpr,
    pub(super) body: ErlExpr,
}

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
    matches!(name, "true" | "false" | "ok" | "error" | "nil")
}

/// Escapes atom text for single-quoted Erlang atom syntax.
///
/// Input is raw atom text. Output is escaped atom text without surrounding
/// quotes. The transformation escapes backslashes and single quotes.
fn escape_quoted_atom(name: &str) -> String {
    name.replace('\\', "\\\\").replace('\'', "\\'")
}
