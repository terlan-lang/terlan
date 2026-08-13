use std::collections::{BTreeMap, HashMap, HashSet};

use crate::terlan_hir::ModuleInterface;
use crate::terlan_syntax::syntax_output::{
    SyntaxAnnotationSchemaEntryOutput, SyntaxAnnotationValueOutput,
};
use crate::terlan_syntax::{
    ebnf::EbnfSourceSpan, parse_module_as_syntax_output, SyntaxClauseOutput,
    SyntaxConfigValueOutput, SyntaxDeclarationPayload, SyntaxExprFieldOutput, SyntaxExprKind,
    SyntaxExprOutput, SyntaxImplConstOutput, SyntaxModuleOutput, SyntaxPatternFieldOutput,
    SyntaxPatternKind, SyntaxPatternOutput,
};

const STEP_LIMIT: usize = 10_000;
const VALUE_SIZE_LIMIT: usize = 1_048_576;

#[derive(Clone, Debug, PartialEq)]
enum ConstValue {
    Int(i64),
    Float(u64),
    Bool(bool),
    Atom(String),
    Binary(String),
    Tuple(Vec<ConstValue>),
    List(Vec<ConstValue>),
    FixedArray(Vec<ConstValue>),
    Map(BTreeMap<String, ConstValue>),
    Record {
        name: String,
        fields: BTreeMap<String, ConstValue>,
    },
    Union {
        name: String,
        arm: String,
        representation: Box<ConstValue>,
    },
}

impl Eq for ConstValue {}

impl ConstValue {
    fn type_name(&self) -> &str {
        match self {
            Self::Int(_) => "Int",
            Self::Float(_) => "Float",
            Self::Bool(_) => "Bool",
            Self::Atom(_) => "Atom",
            Self::Binary(_) => "String",
            Self::Tuple(_) => "Tuple",
            Self::List(_) => "List",
            Self::FixedArray(_) => "FixedArray",
            Self::Map(_) => "Map",
            Self::Record { name, .. } => name,
            Self::Union { name, .. } => name,
        }
    }

    fn size(&self) -> usize {
        match self {
            Self::Int(_) | Self::Float(_) | Self::Bool(_) => 8,
            Self::Atom(value) | Self::Binary(value) => value.len(),
            Self::Tuple(values) | Self::List(values) | Self::FixedArray(values) => {
                values.iter().map(Self::size).sum()
            }
            Self::Map(values) => values
                .iter()
                .map(|(key, value)| key.len() + value.size())
                .sum(),
            Self::Record { name, fields } => {
                name.len()
                    + fields
                        .iter()
                        .map(|(key, value)| key.len() + value.size())
                        .sum::<usize>()
            }
            Self::Union { representation, .. } => representation.size(),
        }
    }

    fn stable_text(&self) -> String {
        match self {
            Self::Int(value) => value.to_string(),
            Self::Float(bits) => f64::from_bits(*bits).to_string(),
            Self::Bool(value) => value.to_string(),
            Self::Atom(value) => format!("Atom[{value:?}]"),
            Self::Binary(value) => format!("{value:?}"),
            Self::Tuple(values) => format!(
                "{{{}}}",
                values
                    .iter()
                    .map(Self::stable_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Self::List(values) => format!(
                "[{}]",
                values
                    .iter()
                    .map(Self::stable_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Self::FixedArray(values) => format!(
                "#[{}]",
                values
                    .iter()
                    .map(Self::stable_text)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Self::Map(values) => format!(
                "#{{{}}}",
                values
                    .iter()
                    .map(|(key, value)| format!("{key}:{}", value.stable_text()))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Self::Record { name, fields } => format!(
                "{name}#{{{}}}",
                fields
                    .iter()
                    .map(|(key, value)| format!("{key}:{}", value.stable_text()))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Self::Union { name, arm, .. } => format!("{name}.{arm}"),
        }
    }
}

#[derive(Clone, Debug)]
struct ConstFunction {
    params: Vec<String>,
    return_type: String,
    body: SyntaxExprOutput,
}

#[derive(Clone, Debug)]
struct ConstDefinition {
    type_name: String,
    expression: SyntaxExprOutput,
    span: EbnfSourceSpan,
    union_arm: Option<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValueLifecycleDiagnostic {
    pub(crate) code: &'static str,
    pub(crate) message: String,
    pub(crate) span: EbnfSourceSpan,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ValueLifecycleReport {
    pub(crate) diagnostics: Vec<ValueLifecycleDiagnostic>,
    pub(crate) fingerprints: BTreeMap<String, String>,
}

struct Evaluator {
    definitions: HashMap<String, ConstDefinition>,
    functions: HashMap<(String, usize), ConstFunction>,
    local_functions: HashSet<(String, usize)>,
    termination: crate::terlan_typeck::CoreTerminationEvidence,
    values: HashMap<String, ConstValue>,
    active: Vec<String>,
    steps: usize,
}

impl Evaluator {
    fn new(module: &SyntaxModuleOutput, interfaces: &HashMap<String, ModuleInterface>) -> Self {
        let mut definitions = HashMap::new();
        let mut functions = HashMap::new();
        let trait_constants = module
            .declarations
            .iter()
            .filter_map(|declaration| match &declaration.payload {
                SyntaxDeclarationPayload::Trait {
                    name, constants, ..
                } => Some((name.clone(), constants.clone())),
                _ => None,
            })
            .collect::<HashMap<_, _>>();
        for declaration in &module.declarations {
            match &declaration.payload {
                SyntaxDeclarationPayload::Constant {
                    name,
                    annotation,
                    value,
                    ..
                } => {
                    definitions.insert(
                        name.clone(),
                        ConstDefinition {
                            type_name: annotation.text.clone(),
                            expression: value.clone(),
                            span: declaration.span,
                            union_arm: None,
                        },
                    );
                }
                SyntaxDeclarationPayload::ConstFunction {
                    name,
                    params,
                    return_type,
                    body,
                    ..
                } => {
                    functions.insert(
                        (name.clone(), params.len()),
                        ConstFunction {
                            params: params.iter().map(|param| param.name.clone()).collect(),
                            return_type: return_type.text.clone(),
                            body: body.clone(),
                        },
                    );
                }
                SyntaxDeclarationPayload::Type {
                    name,
                    representation: Some(representation),
                    valued_arms,
                    ..
                } => {
                    for arm in valued_arms {
                        definitions.insert(
                            format!("{name}.{}", arm.name),
                            ConstDefinition {
                                type_name: representation.text.clone(),
                                expression: arm.value.clone(),
                                span: arm.span,
                                union_arm: Some((name.clone(), arm.name.clone())),
                            },
                        );
                    }
                }
                SyntaxDeclarationPayload::TraitImpl {
                    trait_ref,
                    for_type,
                    constants,
                    is_negative: false,
                    ..
                } => {
                    let trait_name = trait_ref
                        .text
                        .split_once('[')
                        .map(|(name, _)| name)
                        .unwrap_or(&trait_ref.text);
                    if let Some(contract) = trait_constants.get(trait_name) {
                        for required in contract {
                            let provided = constants
                                .iter()
                                .find(|constant| constant.name == required.name);
                            let expression = provided
                                .map(|constant| constant.value.clone())
                                .or_else(|| required.default.clone());
                            if let Some(expression) = expression {
                                definitions.insert(
                                    format!(
                                        "{}.{}",
                                        associated_constant_owner(&trait_ref.text, &for_type.text),
                                        required.name
                                    ),
                                    ConstDefinition {
                                        type_name: required.annotation.text.clone(),
                                        expression,
                                        span: provided
                                            .map(|constant| constant.span)
                                            .unwrap_or(required.span),
                                        union_arm: None,
                                    },
                                );
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        let local_functions = functions.keys().cloned().collect();
        let termination = crate::terlan_typeck::core_const_function_termination_evidence(module);
        let mut evaluator = Self {
            definitions,
            functions,
            local_functions,
            termination,
            values: HashMap::new(),
            active: Vec::new(),
            steps: 0,
        };
        evaluator.load_imported_values(module, interfaces);
        evaluator
    }

    fn load_imported_values(
        &mut self,
        module: &SyntaxModuleOutput,
        interfaces: &HashMap<String, ModuleInterface>,
    ) {
        for (module_name, interface) in interfaces {
            for (name, signature) in &interface.constants {
                if let Some(value) = literal_expr_value(&signature.value) {
                    self.values.insert(format!("{module_name}.{name}"), value);
                }
            }
            for ((name, arity), signature) in &interface.const_functions {
                self.functions.insert(
                    (format!("{module_name}.{name}"), *arity),
                    ConstFunction {
                        params: signature
                            .params
                            .iter()
                            .map(|param| param.name.clone())
                            .collect(),
                        return_type: signature.return_type.clone(),
                        body: signature.body.clone(),
                    },
                );
            }
            for (type_name, union) in &interface.valued_unions {
                for arm in &union.arms {
                    if let Some(representation) = literal_expr_value(&arm.value) {
                        self.values.insert(
                            format!("{module_name}.{type_name}.{}", arm.name),
                            ConstValue::Union {
                                name: format!("{module_name}.{type_name}"),
                                arm: arm.name.clone(),
                                representation: Box::new(representation),
                            },
                        );
                    }
                }
            }
            for (name, signature) in &interface.associated_constants {
                if let Some(value) = literal_expr_value(&signature.value) {
                    self.values.insert(name.clone(), value.clone());
                    self.values.insert(format!("{module_name}.{name}"), value);
                }
            }
        }

        for declaration in &module.declarations {
            let SyntaxDeclarationPayload::Import {
                module_name, items, ..
            } = &declaration.payload
            else {
                continue;
            };
            let Some(interface) = interfaces.get(module_name) else {
                continue;
            };
            for item in items {
                if item.name == "*" {
                    for (name, signature) in &interface.constants {
                        if let Some(value) = literal_expr_value(&signature.value) {
                            self.values.insert(name.clone(), value);
                        }
                    }
                    for ((name, arity), signature) in &interface.const_functions {
                        self.functions.insert(
                            (name.clone(), *arity),
                            const_function_from_signature(signature),
                        );
                    }
                    for (type_name, union) in &interface.valued_unions {
                        self.load_imported_union(module_name, type_name, union);
                    }
                    continue;
                }
                let local_name = item.as_alias.as_ref().unwrap_or(&item.name);
                if let Some(signature) = interface.constants.get(&item.name) {
                    if let Some(value) = literal_expr_value(&signature.value) {
                        self.values.insert(local_name.clone(), value);
                    }
                }
                for ((name, arity), signature) in &interface.const_functions {
                    if name == &item.name {
                        self.functions.insert(
                            (local_name.clone(), *arity),
                            const_function_from_signature(signature),
                        );
                    }
                }
                if let Some(union) = interface.valued_unions.get(&item.name) {
                    self.load_imported_union(module_name, local_name, union);
                }
            }
        }
    }

    fn load_imported_union(
        &mut self,
        module_name: &str,
        local_type_name: &str,
        union: &crate::terlan_hir::ValuedUnionSignature,
    ) {
        for arm in &union.arms {
            if let Some(representation) = literal_expr_value(&arm.value) {
                self.values.insert(
                    format!("{local_type_name}.{}", arm.name),
                    ConstValue::Union {
                        name: format!("{module_name}.{}", union.name),
                        arm: arm.name.clone(),
                        representation: Box::new(representation),
                    },
                );
            }
        }
    }

    fn tick(&mut self, span: EbnfSourceSpan) -> Result<(), ValueLifecycleDiagnostic> {
        self.steps += 1;
        if self.steps > STEP_LIMIT {
            return Err(diagnostic(
                "CONST_EVALUATOR_EXHAUSTED",
                "constant evaluation exceeded the deterministic step limit",
                span,
            ));
        }
        Ok(())
    }

    fn evaluate_named(&mut self, name: &str) -> Result<ConstValue, ValueLifecycleDiagnostic> {
        if let Some(value) = self.values.get(name) {
            return Ok(value.clone());
        }
        let Some(definition) = self.definitions.get(name).cloned() else {
            return Err(diagnostic(
                "UNKNOWN_CONSTANT",
                format!("cannot resolve constant `{name}`"),
                EbnfSourceSpan::default(),
            ));
        };
        if let Some(position) = self.active.iter().position(|active| active == name) {
            let mut cycle = self.active[position..].to_vec();
            cycle.push(name.to_string());
            return Err(diagnostic(
                "CONST_CYCLE",
                format!("constant cycle detected: {}", cycle.join(" -> ")),
                definition.span,
            ));
        }
        self.active.push(name.to_string());
        let result = self.evaluate_expr(&definition.expression, &HashMap::new());
        self.active.pop();
        let value = result?;
        ensure_type(&definition.type_name, &value, definition.span)?;
        if value.size() > VALUE_SIZE_LIMIT {
            return Err(diagnostic(
                "CONST_EVALUATOR_EXHAUSTED",
                "constant value exceeded the deterministic output-size limit",
                definition.span,
            ));
        }
        let value = if let Some((union, arm)) = definition.union_arm {
            ConstValue::Union {
                name: union,
                arm,
                representation: Box::new(value),
            }
        } else {
            value
        };
        self.values.insert(name.to_string(), value.clone());
        Ok(value)
    }

    fn evaluate_expr(
        &mut self,
        expr: &SyntaxExprOutput,
        locals: &HashMap<String, ConstValue>,
    ) -> Result<ConstValue, ValueLifecycleDiagnostic> {
        self.tick(expr.span)?;
        match expr.kind {
            SyntaxExprKind::Int => expr
                .text
                .as_deref()
                .and_then(|text| text.parse().ok())
                .map(ConstValue::Int)
                .ok_or_else(|| diagnostic("CONST_INVALID_LITERAL", "invalid integer", expr.span)),
            SyntaxExprKind::Float => expr
                .text
                .as_deref()
                .and_then(|text| text.parse::<f64>().ok())
                .map(|value| ConstValue::Float(value.to_bits()))
                .ok_or_else(|| diagnostic("CONST_INVALID_LITERAL", "invalid float", expr.span)),
            SyntaxExprKind::Binary => Ok(ConstValue::Binary(expr.text.clone().unwrap_or_default())),
            SyntaxExprKind::Atom => match expr.text.as_deref() {
                Some("true") => Ok(ConstValue::Bool(true)),
                Some("false") => Ok(ConstValue::Bool(false)),
                Some(value) => Ok(ConstValue::Atom(value.to_string())),
                None => Err(diagnostic(
                    "CONST_INVALID_LITERAL",
                    "invalid atom",
                    expr.span,
                )),
            },
            SyntaxExprKind::Var => {
                let name = expr.text.as_deref().unwrap_or_default();
                if name == "true" {
                    return Ok(ConstValue::Bool(true));
                }
                if name == "false" {
                    return Ok(ConstValue::Bool(false));
                }
                if let Some(value) = locals.get(name) {
                    return Ok(value.clone());
                }
                self.evaluate_named(name)
            }
            SyntaxExprKind::FieldAccess => {
                let name = qualified_expr_name(expr)
                    .ok_or_else(|| not_const(expr.span, "dynamic field access"))?;
                if self.definitions.contains_key(&name) || self.values.contains_key(&name) {
                    return self.evaluate_named(&name);
                }
                let owner = self.evaluate_expr(required_child(expr, 0)?, locals)?;
                let field = expr.text.as_deref().unwrap_or_default();
                match owner {
                    ConstValue::Map(fields) | ConstValue::Record { fields, .. } => {
                        fields.get(field).cloned().ok_or_else(|| {
                            diagnostic(
                                "CONST_FIELD_MISSING",
                                format!("constant value has no field `{field}`"),
                                expr.span,
                            )
                        })
                    }
                    _ => Err(not_const(
                        expr.span,
                        "field access on a non-aggregate constant",
                    )),
                }
            }
            SyntaxExprKind::Tuple => self
                .evaluate_sequence(&expr.children, locals)
                .map(ConstValue::Tuple),
            SyntaxExprKind::List => self
                .evaluate_sequence(&expr.children, locals)
                .map(ConstValue::List),
            SyntaxExprKind::FixedArray => self
                .evaluate_sequence(&expr.children, locals)
                .map(ConstValue::FixedArray),
            SyntaxExprKind::Map => {
                let mut values = BTreeMap::new();
                for field in &expr.fields {
                    values.insert(field.key.clone(), self.evaluate_expr(&field.value, locals)?);
                }
                Ok(ConstValue::Map(values))
            }
            SyntaxExprKind::RecordConstruct => {
                let mut fields = BTreeMap::new();
                for field in &expr.fields {
                    fields.insert(field.key.clone(), self.evaluate_expr(&field.value, locals)?);
                }
                Ok(ConstValue::Record {
                    name: expr.text.clone().unwrap_or_default(),
                    fields,
                })
            }
            SyntaxExprKind::Index => {
                let collection = self.evaluate_expr(required_child(expr, 0)?, locals)?;
                let index = self.evaluate_expr(required_child(expr, 1)?, locals)?;
                let ConstValue::Int(index) = index else {
                    return Err(diagnostic(
                        "CONST_INDEX_TYPE",
                        "constant aggregate index must be Int",
                        expr.span,
                    ));
                };
                let index = usize::try_from(index).map_err(|_| {
                    diagnostic(
                        "CONST_INDEX_BOUNDS",
                        "constant aggregate index is out of bounds",
                        expr.span,
                    )
                })?;
                match collection {
                    ConstValue::Tuple(values)
                    | ConstValue::List(values)
                    | ConstValue::FixedArray(values) => {
                        values.get(index).cloned().ok_or_else(|| {
                            diagnostic(
                                "CONST_INDEX_BOUNDS",
                                "constant aggregate index is out of bounds",
                                expr.span,
                            )
                        })
                    }
                    _ => Err(not_const(expr.span, "indexing a non-sequence constant")),
                }
            }
            SyntaxExprKind::UnaryOp => self.evaluate_unary(expr, locals),
            SyntaxExprKind::BinaryOp => self.evaluate_binary(expr, locals),
            SyntaxExprKind::Call | SyntaxExprKind::FunctionCall => self.evaluate_call(expr, locals),
            SyntaxExprKind::Let => self.evaluate_let(expr, locals),
            SyntaxExprKind::Case => self.evaluate_case(expr, locals),
            SyntaxExprKind::Sequence => expr
                .children
                .iter()
                .try_fold(ConstValue::Tuple(Vec::new()), |_, child| {
                    self.evaluate_expr(child, locals)
                }),
            _ => Err(not_const(expr.span, const_forbidden_form(expr.kind))),
        }
    }

    fn evaluate_sequence(
        &mut self,
        expressions: &[SyntaxExprOutput],
        locals: &HashMap<String, ConstValue>,
    ) -> Result<Vec<ConstValue>, ValueLifecycleDiagnostic> {
        expressions
            .iter()
            .map(|expr| self.evaluate_expr(expr, locals))
            .collect()
    }

    fn evaluate_unary(
        &mut self,
        expr: &SyntaxExprOutput,
        locals: &HashMap<String, ConstValue>,
    ) -> Result<ConstValue, ValueLifecycleDiagnostic> {
        let value = self.evaluate_expr(required_child(expr, 0)?, locals)?;
        match (expr.operator.as_deref(), value) {
            (Some("-"), ConstValue::Int(value)) => value
                .checked_neg()
                .map(ConstValue::Int)
                .ok_or_else(|| diagnostic("CONST_OVERFLOW", "integer overflow", expr.span)),
            (Some("not"), ConstValue::Bool(value)) => Ok(ConstValue::Bool(!value)),
            _ => Err(not_const(expr.span, "unsupported const unary operation")),
        }
    }

    fn evaluate_binary(
        &mut self,
        expr: &SyntaxExprOutput,
        locals: &HashMap<String, ConstValue>,
    ) -> Result<ConstValue, ValueLifecycleDiagnostic> {
        let left = self.evaluate_expr(required_child(expr, 0)?, locals)?;
        let right = self.evaluate_expr(required_child(expr, 1)?, locals)?;
        match (expr.operator.as_deref(), left, right) {
            (Some("+"), ConstValue::Int(a), ConstValue::Int(b)) => {
                checked_int(a.checked_add(b), expr.span)
            }
            (Some("-"), ConstValue::Int(a), ConstValue::Int(b)) => {
                checked_int(a.checked_sub(b), expr.span)
            }
            (Some("*"), ConstValue::Int(a), ConstValue::Int(b)) => {
                checked_int(a.checked_mul(b), expr.span)
            }
            (Some("div" | "/"), ConstValue::Int(_), ConstValue::Int(0)) => Err(diagnostic(
                "CONST_DIVISION_BY_ZERO",
                "division by zero",
                expr.span,
            )),
            (Some("div" | "/"), ConstValue::Int(a), ConstValue::Int(b)) => {
                checked_int(a.checked_div(b), expr.span)
            }
            (Some("rem"), ConstValue::Int(_), ConstValue::Int(0)) => Err(diagnostic(
                "CONST_DIVISION_BY_ZERO",
                "remainder by zero",
                expr.span,
            )),
            (Some("rem"), ConstValue::Int(a), ConstValue::Int(b)) => {
                checked_int(a.checked_rem(b), expr.span)
            }
            (Some("==" | "==="), a, b) => Ok(ConstValue::Bool(a == b)),
            (Some("!=" | "!=="), a, b) => Ok(ConstValue::Bool(a != b)),
            (Some("and"), ConstValue::Bool(a), ConstValue::Bool(b)) => Ok(ConstValue::Bool(a && b)),
            (Some("or"), ConstValue::Bool(a), ConstValue::Bool(b)) => Ok(ConstValue::Bool(a || b)),
            (Some("<"), ConstValue::Int(a), ConstValue::Int(b)) => Ok(ConstValue::Bool(a < b)),
            (Some(">"), ConstValue::Int(a), ConstValue::Int(b)) => Ok(ConstValue::Bool(a > b)),
            (Some("<="), ConstValue::Int(a), ConstValue::Int(b)) => Ok(ConstValue::Bool(a <= b)),
            (Some(">="), ConstValue::Int(a), ConstValue::Int(b)) => Ok(ConstValue::Bool(a >= b)),
            _ => Err(not_const(expr.span, "unsupported const binary operation")),
        }
    }

    fn evaluate_call(
        &mut self,
        expr: &SyntaxExprOutput,
        locals: &HashMap<String, ConstValue>,
    ) -> Result<ConstValue, ValueLifecycleDiagnostic> {
        let callee = required_child(expr, 0)?;
        let local_name = qualified_expr_name(callee)
            .ok_or_else(|| not_const(expr.span, "dynamic function call"))?;
        let name = expr
            .remote
            .as_ref()
            .map(|module| format!("{module}.{local_name}"))
            .unwrap_or(local_name);
        let args = self.evaluate_sequence(&expr.children[1..], locals)?;
        let Some(function) = self.functions.get(&(name.clone(), args.len())).cloned() else {
            return Err(not_const(
                expr.span,
                format!("ordinary function call `{name}`"),
            ));
        };
        let call_name = format!("{name}/{}", args.len());
        let key = (name.clone(), args.len());
        if self.local_functions.contains(&key) {
            self.termination
                .require_total(&name, args.len())
                .map_err(|message| {
                    diagnostic(
                        "CONST_TOTALITY_UNPROVEN",
                        format!("const-function call requires proven termination: {message}"),
                        expr.span,
                    )
                })?;
        } else if self.active.iter().any(|active| active == &call_name) {
            return Err(diagnostic(
                "CONST_TOTALITY_UNPROVEN",
                "recursive imported const function lacks validated termination evidence",
                expr.span,
            ));
        }
        self.active.push(call_name);
        let call_locals = function
            .params
            .into_iter()
            .zip(args)
            .collect::<HashMap<_, _>>();
        let value = self.evaluate_expr(&function.body, &call_locals);
        self.active.pop();
        let value = value?;
        ensure_type(&function.return_type, &value, expr.span)?;
        Ok(value)
    }

    fn evaluate_let(
        &mut self,
        expr: &SyntaxExprOutput,
        locals: &HashMap<String, ConstValue>,
    ) -> Result<ConstValue, ValueLifecycleDiagnostic> {
        let binding_count = expr.arity.min(expr.patterns.len());
        let mut scope = locals.clone();
        for index in 0..binding_count {
            let value = self.evaluate_expr(required_child(expr, index)?, &scope)?;
            if !match_pattern(&expr.patterns[index], &value, &mut scope) {
                return Err(diagnostic(
                    "CONST_MATCH_ASSERTION_FAILED",
                    "refutable let pattern failed during constant evaluation",
                    expr.span,
                ));
            }
        }
        let body = expr.children.get(binding_count).ok_or_else(|| {
            diagnostic(
                "CONST_INVALID_EXPRESSION",
                "const let requires a body",
                expr.span,
            )
        })?;
        self.evaluate_expr(body, &scope)
    }

    fn evaluate_case(
        &mut self,
        expr: &SyntaxExprOutput,
        locals: &HashMap<String, ConstValue>,
    ) -> Result<ConstValue, ValueLifecycleDiagnostic> {
        let value = self.evaluate_expr(required_child(expr, 0)?, locals)?;
        for clause in &expr.clauses {
            let mut scope = locals.clone();
            if clause
                .patterns
                .first()
                .is_some_and(|pattern| match_pattern(pattern, &value, &mut scope))
            {
                if let Some(guard) = &clause.guard {
                    if self.evaluate_expr(guard, &scope)? != ConstValue::Bool(true) {
                        continue;
                    }
                }
                return self.evaluate_expr(&clause.body, &scope);
            }
        }
        Err(diagnostic(
            "CONST_MATCH_ASSERTION_FAILED",
            "no const case clause matched",
            expr.span,
        ))
    }
}

pub(crate) fn evaluate_and_substitute_module_constants_with_interfaces(
    module: &mut SyntaxModuleOutput,
    interfaces: &HashMap<String, ModuleInterface>,
) -> ValueLifecycleReport {
    let mut evaluator = Evaluator::new(module, interfaces);
    let mut report = ValueLifecycleReport {
        diagnostics: validate_constant_namespaces(module),
        fingerprints: BTreeMap::new(),
    };
    report
        .diagnostics
        .extend(validate_valued_union_case_exhaustiveness(module));
    let names = evaluator.definitions.keys().cloned().collect::<Vec<_>>();
    for name in names {
        match evaluator.evaluate_named(&name) {
            Ok(value) => {
                report
                    .fingerprints
                    .insert(name, stable_fingerprint(&value.stable_text()));
            }
            Err(error) => report.diagnostics.push(error),
        }
    }
    let local_values = evaluator.values.clone();
    for (name, value) in local_values {
        if evaluator.definitions.contains_key(&name) {
            evaluator
                .values
                .insert(format!("{}.{}", module.module_name, name), value.clone());
            if let Some(short_module) = module.module_name.rsplit('.').next() {
                evaluator
                    .values
                    .insert(format!("{short_module}.{name}"), value);
            }
        }
    }
    report
        .diagnostics
        .extend(validate_valued_unions(module, &evaluator.values));
    report
        .diagnostics
        .extend(validate_trait_constants(module, &mut evaluator));
    lower_checked_valued_union_parsing(module, &evaluator.values, &mut report.diagnostics);
    report
        .diagnostics
        .extend(validate_nominal_valued_union_uses(module));
    report
        .diagnostics
        .extend(validate_runtime_constant_reflection(module));
    report.diagnostics.extend(resolve_const_generic_arguments(
        module,
        interfaces,
        &mut evaluator,
    ));
    report
        .diagnostics
        .extend(validate_forbidden_constant_contexts(module, &evaluator));
    if report.diagnostics.is_empty() {
        materialize_trait_constant_defaults(module, &evaluator.values);
        substitute_module(module, &mut evaluator, &mut report.diagnostics);
    }
    report
}

fn materialize_trait_constant_defaults(
    module: &mut SyntaxModuleOutput,
    values: &HashMap<String, ConstValue>,
) {
    let contracts = module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Trait {
                name, constants, ..
            } => Some((name.clone(), constants.clone())),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    for declaration in &mut module.declarations {
        let SyntaxDeclarationPayload::TraitImpl {
            trait_ref,
            for_type,
            constants,
            is_negative: false,
            ..
        } = &mut declaration.payload
        else {
            continue;
        };
        let trait_name = trait_ref.text.split('[').next().unwrap_or(&trait_ref.text);
        let Some(contract) = contracts.get(trait_name) else {
            continue;
        };
        let owner = associated_constant_owner(&trait_ref.text, &for_type.text);
        for required in contract {
            if constants
                .iter()
                .any(|constant| constant.name == required.name)
            {
                continue;
            }
            let Some(value) = values.get(&format!("{owner}.{}", required.name)) else {
                continue;
            };
            constants.push(SyntaxImplConstOutput {
                name: required.name.clone(),
                annotation: Some(required.annotation.clone()),
                value: value_to_expr(value, required.span),
                span: required.span,
            });
        }
    }
}

#[path = "value_lifecycle/checked_conversion.rs"]
mod checked_conversion;
#[path = "value_lifecycle/const_generics.rs"]
mod const_generics;
#[path = "value_lifecycle/substitution.rs"]
mod substitution;
#[path = "value_lifecycle/validation.rs"]
mod validation;

use checked_conversion::*;
use const_generics::*;
use substitution::*;
use validation::{
    associated_constant_owner, validate_constant_namespaces, validate_forbidden_constant_contexts,
    validate_nominal_valued_union_uses, validate_runtime_constant_reflection,
    validate_trait_constants, validate_valued_union_case_exhaustiveness, validate_valued_unions,
};
pub(crate) use validation::{
    evaluate_and_substitute_module_constants, expression_is_const_safe,
    module_requires_value_lifecycle_pass,
};

#[cfg(test)]
#[path = "value_lifecycle_test.rs"]
#[cfg(test)]
mod value_lifecycle_test;
