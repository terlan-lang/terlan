use std::collections::{BTreeMap, BTreeSet};

use crate::terlan_syntax::{
    ebnf::EbnfSourceSpan, SyntaxClauseOutput, SyntaxDeclarationPayload, SyntaxExprKind,
    SyntaxExprOutput, SyntaxHtmlNodeOutput, SyntaxModuleOutput, SyntaxParamOutput,
};
use terlan_runtime_abi::{BoundaryError, ErrorDomain};

use super::{DiagSeverity, Diagnostic};

mod patterns;

/// Stable identity assigned to one immutable lexical binding.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
/// Data describing core binding id.
pub struct CoreBindingId(pub u64);

/// Stable identity assigned to one compiler-defined lexical region.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
/// Data describing core binding region id.
pub struct CoreBindingRegionId(pub u64);

/// Source family that introduced one binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreBindingKind {
    Parameter,
    Pattern,
    Alias,
    StringCapture,
}

/// One exact immutable binding carried by checked CoreIR.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CoreBindingIdentity {
    pub id: CoreBindingId,
    pub region: CoreBindingRegionId,
    pub name: String,
    pub kind: CoreBindingKind,
    pub path: String,
    pub span_start: usize,
    pub span_end: usize,
}

/// One variable occurrence resolved to an exact immutable binding.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CoreBindingReference {
    pub binding: CoreBindingId,
    pub name: String,
    pub path: String,
    pub span_start: usize,
    pub span_end: usize,
}

/// Backend-neutral binding evidence carried by CoreIR.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CoreBindingIdentityEvidence {
    pub bindings: Vec<CoreBindingIdentity>,
    pub references: Vec<CoreBindingReference>,
    pub fingerprint: String,
}

impl Default for CoreBindingIdentityEvidence {
    fn default() -> Self {
        Self {
            bindings: Vec::new(),
            references: Vec::new(),
            fingerprint: evidence_fingerprint(&[], &[]),
        }
    }
}

impl CoreBindingIdentityEvidence {
    /// Validates uniqueness and the deterministic evidence fingerprint.
    pub fn validate(&self) -> Result<(), BoundaryError> {
        self.validate_untyped().map_err(|error| {
            BoundaryError::message(
                ErrorDomain::CompilerPhase,
                "validate CoreIR binding identity evidence",
                error,
            )
        })
    }

    fn validate_untyped(&self) -> Result<(), String> {
        let mut ids = BTreeSet::new();
        for binding in &self.bindings {
            if binding.id.0 == 0 || binding.region.0 == 0 {
                return Err("error[core.binding_identity]: zero binding identity".to_string());
            }
            if !ids.insert(binding.id) {
                return Err(format!(
                    "error[core.binding_identity]: duplicate binding identity {:016x}",
                    binding.id.0
                ));
            }
        }
        for reference in &self.references {
            if !ids.contains(&reference.binding) {
                return Err(format!(
                    "error[core.binding_identity]: reference `{}` targets missing identity {:016x}",
                    reference.name, reference.binding.0
                ));
            }
        }
        let expected = evidence_fingerprint(&self.bindings, &self.references);
        if self.fingerprint != expected {
            return Err(format!(
                "error[core.binding_identity]: stale fingerprint `{}` expected `{expected}`",
                self.fingerprint
            ));
        }
        Ok(())
    }

    /// Returns all debugger-visible locals in one exact lexical region.
    pub fn debugger_locals(&self, region: CoreBindingRegionId) -> Vec<&CoreBindingIdentity> {
        self.bindings
            .iter()
            .filter(|binding| binding.region == region)
            .collect()
    }

    /// Returns declaration and use records for an exact binding.
    pub fn references_for(
        &self,
        binding: CoreBindingId,
    ) -> (Option<&CoreBindingIdentity>, Vec<&CoreBindingReference>) {
        (
            self.bindings.iter().find(|item| item.id == binding),
            self.references
                .iter()
                .filter(|item| item.binding == binding)
                .collect(),
        )
    }
}

/// One duplicate binding rejected before string-keyed type inference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingCollision {
    pub name: String,
    pub original: CoreBindingId,
    pub region: CoreBindingRegionId,
    pub path: String,
    pub span: EbnfSourceSpan,
    pub suggested_name: String,
}

impl BindingCollision {
    /// Returns diagnostic.
    pub fn diagnostic(&self) -> Diagnostic {
        Diagnostic {
            span: self.span.into(),
            message: format!(
                "error[binding.same_region]: `{}` is already bound in this lexical region; use `{}` for a distinct immutable binding",
                self.name, self.suggested_name
            ),
            severity: DiagSeverity::Error,
        }
    }
}

/// Complete result of deterministic binding-region analysis.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BindingAnalysis {
    pub evidence: CoreBindingIdentityEvidence,
    pub collisions: Vec<BindingCollision>,
}

impl BindingAnalysis {
    /// Returns diagnostics.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.collisions
            .iter()
            .map(BindingCollision::diagnostic)
            .collect()
    }
}

#[derive(Clone, Default)]
struct LexicalEnvironment {
    visible: BTreeMap<String, CoreBindingId>,
}

struct Region {
    id: CoreBindingRegionId,
    path: String,
    local: BTreeMap<String, CoreBindingId>,
    visible_names: BTreeSet<String>,
}

struct Analyzer<'a> {
    module: &'a str,
    bindings: Vec<CoreBindingIdentity>,
    references: Vec<CoreBindingReference>,
    collisions: Vec<BindingCollision>,
}

/// Analyzes immutable lexical bindings after shape and macro expansion.
pub fn analyze_syntax_bindings(module: &SyntaxModuleOutput) -> BindingAnalysis {
    let mut analyzer = Analyzer {
        module: &module.module_name,
        bindings: Vec::new(),
        references: Vec::new(),
        collisions: Vec::new(),
    };
    let mut declaration_occurrences = BTreeMap::<String, usize>::new();
    for (index, declaration) in module.declarations.iter().enumerate() {
        let base_path = declaration_path(&declaration.payload, index);
        let occurrence = declaration_occurrences
            .entry(base_path.clone())
            .or_default();
        let path = if *occurrence == 0 {
            base_path
        } else {
            format!("{base_path}:overload:{occurrence}")
        };
        *occurrence += 1;
        analyzer.declaration(&declaration.payload, declaration.span, &path);
    }
    let fingerprint = evidence_fingerprint(&analyzer.bindings, &analyzer.references);
    BindingAnalysis {
        evidence: CoreBindingIdentityEvidence {
            bindings: analyzer.bindings,
            references: analyzer.references,
            fingerprint,
        },
        collisions: analyzer.collisions,
    }
}

fn declaration_path(declaration: &SyntaxDeclarationPayload, index: usize) -> String {
    match declaration {
        SyntaxDeclarationPayload::Constant { name, .. } => format!("constant:{name}"),
        SyntaxDeclarationPayload::ConstFunction { name, params, .. } => {
            format!("const_function:{name}/{}", params.len())
        }
        SyntaxDeclarationPayload::Type { name, .. } => format!("type:{name}"),
        SyntaxDeclarationPayload::Struct { name, .. } => format!("struct:{name}"),
        SyntaxDeclarationPayload::Constructor { name, clauses, .. } => {
            let arity = clauses.first().map_or(0, |clause| clause.params.len());
            format!("constructor:{name}/{arity}")
        }
        SyntaxDeclarationPayload::Function {
            name,
            params,
            is_macro,
            ..
        } => format!(
            "{}:{name}/{}",
            if *is_macro { "macro" } else { "function" },
            params.len()
        ),
        SyntaxDeclarationPayload::Method { name, params, .. } => {
            format!("method:{name}/{}", params.len() + 1)
        }
        SyntaxDeclarationPayload::Trait { name, .. } => format!("trait:{name}"),
        SyntaxDeclarationPayload::TraitImpl {
            trait_ref,
            for_type,
            ..
        } => format!("impl:{}:{}", trait_ref.text, for_type.text),
        SyntaxDeclarationPayload::Template { name, .. } => format!("template:{name}"),
        SyntaxDeclarationPayload::Import { .. }
        | SyntaxDeclarationPayload::Export { .. }
        | SyntaxDeclarationPayload::AnnotationSchema { .. }
        | SyntaxDeclarationPayload::Config { .. }
        | SyntaxDeclarationPayload::Raw { .. } => format!("declaration:{index}"),
    }
}

impl Analyzer<'_> {
    fn declaration(
        &mut self,
        declaration: &SyntaxDeclarationPayload,
        span: EbnfSourceSpan,
        path: &str,
    ) {
        match declaration {
            SyntaxDeclarationPayload::ConstFunction {
                name, params, body, ..
            } => self.parameter_body(params, body, span, &format!("{path}:const:{name}")),
            SyntaxDeclarationPayload::Function {
                name,
                params,
                clauses,
                ..
            } => self.callable_clauses(params, clauses, &format!("{path}:function:{name}")),
            SyntaxDeclarationPayload::Method {
                receiver,
                name,
                params,
                clauses,
                ..
            } => {
                let mut all_params = vec![receiver.as_ref().clone()];
                all_params.extend(params.iter().cloned());
                self.callable_clauses(&all_params, clauses, &format!("{path}:method:{name}"));
            }
            SyntaxDeclarationPayload::Constructor { name, clauses, .. } => {
                for (index, clause) in clauses.iter().enumerate() {
                    let region_path = format!("{path}:constructor:{name}:clause:{index}");
                    let mut environment = LexicalEnvironment::default();
                    let mut region = self.region(&region_path, environment.clone());
                    for (param_index, param) in clause.params.iter().enumerate() {
                        self.expression(
                            param.default.as_ref(),
                            &mut environment,
                            &mut region,
                            &format!("{region_path}:default:{param_index}"),
                            false,
                        );
                        self.bind_name(
                            &param.name,
                            CoreBindingKind::Parameter,
                            param.span,
                            &format!("{region_path}:param:{param_index}"),
                            &mut environment,
                            &mut region,
                        );
                    }
                    self.expression(
                        Some(&clause.body),
                        &mut environment,
                        &mut region,
                        &format!("{region_path}:body"),
                        true,
                    );
                }
            }
            SyntaxDeclarationPayload::Trait { methods, .. } => {
                for (index, method) in methods.iter().enumerate() {
                    if let Some(body) = &method.default_body {
                        self.parameter_body(
                            &method.params,
                            body,
                            method.span,
                            &format!("{path}:trait_method:{index}:{}", method.name),
                        );
                    }
                }
            }
            SyntaxDeclarationPayload::TraitImpl { methods, .. } => {
                for (index, method) in methods.iter().enumerate() {
                    self.callable_clauses(
                        &method.params,
                        &method.clauses,
                        &format!("{path}:impl_method:{index}:{}", method.name),
                    );
                }
            }
            SyntaxDeclarationPayload::Constant { value, .. } => {
                self.root_expression(value, &format!("{path}:constant"));
            }
            SyntaxDeclarationPayload::Type { valued_arms, .. } => {
                for (index, arm) in valued_arms.iter().enumerate() {
                    self.root_expression(&arm.value, &format!("{path}:valued_arm:{index}"));
                }
            }
            SyntaxDeclarationPayload::Struct { fields, .. } => {
                for (index, field) in fields.iter().enumerate() {
                    if let Some(default) = &field.default {
                        self.root_expression(default, &format!("{path}:field:{index}"));
                    }
                }
            }
            SyntaxDeclarationPayload::Template { props, .. } => {
                for (index, prop) in props.iter().enumerate() {
                    if let Some(default) = &prop.default {
                        self.root_expression(default, &format!("{path}:prop:{index}"));
                    }
                }
            }
            SyntaxDeclarationPayload::Import { .. }
            | SyntaxDeclarationPayload::Export { .. }
            | SyntaxDeclarationPayload::AnnotationSchema { .. }
            | SyntaxDeclarationPayload::Config { .. }
            | SyntaxDeclarationPayload::Raw { .. } => {}
        }
    }

    fn callable_clauses(
        &mut self,
        params: &[SyntaxParamOutput],
        clauses: &[crate::terlan_syntax::SyntaxFunctionClauseOutput],
        path: &str,
    ) {
        for (index, clause) in clauses.iter().enumerate() {
            let region_path = format!("{path}:clause:{index}");
            let mut environment = LexicalEnvironment::default();
            let mut region = self.region(&region_path, environment.clone());
            if clause.patterns.is_empty() {
                self.bind_params(params, &mut environment, &mut region, &region_path);
            } else {
                for (pattern_index, pattern) in clause.patterns.iter().enumerate() {
                    self.bind_pattern(
                        pattern,
                        clause.span,
                        &format!("{region_path}:head:{pattern_index}"),
                        &mut environment,
                        &mut region,
                    );
                }
            }
            self.expression(
                clause.guard.as_ref(),
                &mut environment,
                &mut region,
                &format!("{region_path}:guard"),
                false,
            );
            self.expression(
                Some(&clause.body),
                &mut environment,
                &mut region,
                &format!("{region_path}:body"),
                true,
            );
        }
    }

    fn parameter_body(
        &mut self,
        params: &[SyntaxParamOutput],
        body: &SyntaxExprOutput,
        span: EbnfSourceSpan,
        path: &str,
    ) {
        let mut environment = LexicalEnvironment::default();
        let mut region = self.region(path, environment.clone());
        self.bind_params(params, &mut environment, &mut region, path);
        self.expression(
            Some(body),
            &mut environment,
            &mut region,
            &format!("{path}:body:{}:{}", span.start, span.end),
            true,
        );
    }

    fn bind_params(
        &mut self,
        params: &[SyntaxParamOutput],
        environment: &mut LexicalEnvironment,
        region: &mut Region,
        path: &str,
    ) {
        for (index, param) in params.iter().enumerate() {
            self.expression(
                param.default.as_ref(),
                environment,
                region,
                &format!("{path}:default:{index}"),
                false,
            );
            self.bind_name(
                &param.name,
                CoreBindingKind::Parameter,
                param.span,
                &format!("{path}:param:{index}"),
                environment,
                region,
            );
        }
    }

    fn root_expression(&mut self, expression: &SyntaxExprOutput, path: &str) {
        let mut environment = LexicalEnvironment::default();
        let mut region = self.region(path, environment.clone());
        self.expression(Some(expression), &mut environment, &mut region, path, false);
    }

    fn expression(
        &mut self,
        expression: Option<&SyntaxExprOutput>,
        environment: &mut LexicalEnvironment,
        region: &mut Region,
        path: &str,
        reuse_region: bool,
    ) {
        let Some(expression) = expression else {
            return;
        };
        if expression.kind == SyntaxExprKind::Var {
            if let Some(name) = expression.text.as_deref() {
                self.reference(name, expression.span, path, environment);
            }
            return;
        }
        match expression.kind {
            SyntaxExprKind::Let => {
                self.let_expression(expression, environment, region, path, reuse_region);
                return;
            }
            SyntaxExprKind::Case => {
                if let Some(scrutinee) = expression.children.first() {
                    self.expression(
                        Some(scrutinee),
                        environment,
                        region,
                        &format!("{path}:scrutinee"),
                        false,
                    );
                }
                self.clauses(&expression.clauses, environment, path, "case");
                return;
            }
            SyntaxExprKind::Try => {
                if let Some(body) = expression.children.first() {
                    self.expression(
                        Some(body),
                        environment,
                        region,
                        &format!("{path}:try_body"),
                        false,
                    );
                }
                self.clauses(&expression.clauses, environment, path, "try_of");
                self.clauses(&expression.catch_clauses, environment, path, "catch");
                if let Some(after) = &expression.try_after {
                    self.nested_expression(
                        &after.trigger,
                        environment,
                        &format!("{path}:after_trigger"),
                    );
                    self.nested_expression(&after.body, environment, &format!("{path}:after_body"));
                }
                return;
            }
            SyntaxExprKind::If => {
                self.clauses(&expression.clauses, environment, path, "if");
                return;
            }
            SyntaxExprKind::Fun => {
                self.clauses(&expression.clauses, environment, path, "lambda");
                return;
            }
            SyntaxExprKind::ListComprehension => {
                self.comprehension(expression, environment, path);
                return;
            }
            _ => {}
        }
        for (index, child) in expression.children.iter().enumerate() {
            self.expression(
                Some(child),
                environment,
                region,
                &format!("{path}:child:{index}"),
                false,
            );
        }
        for (index, field) in expression.fields.iter().enumerate() {
            self.expression(
                Some(&field.value),
                environment,
                region,
                &format!("{path}:field:{index}:{}", field.key),
                false,
            );
        }
        self.clauses(&expression.clauses, environment, path, "clause");
        self.clauses(&expression.catch_clauses, environment, path, "catch");
        if let Some(after) = &expression.try_after {
            self.nested_expression(
                &after.trigger,
                environment,
                &format!("{path}:after_trigger"),
            );
            self.nested_expression(&after.body, environment, &format!("{path}:after_body"));
        }
        for (index, node) in expression.html_nodes.iter().enumerate() {
            self.html_node(node, environment, &format!("{path}:html:{index}"));
        }
    }

    fn let_expression(
        &mut self,
        expression: &SyntaxExprOutput,
        outer: &mut LexicalEnvironment,
        outer_region: &mut Region,
        path: &str,
        reuse_region: bool,
    ) {
        let fallback_environment = outer.clone();
        let mut owned_environment;
        let mut owned_region;
        let (environment, region) = if reuse_region {
            (outer, outer_region)
        } else {
            owned_environment = outer.clone();
            owned_region = self.region(path, owned_environment.clone());
            (&mut owned_environment, &mut owned_region)
        };
        for (index, pattern) in expression.patterns.iter().enumerate() {
            if let Some(value) = expression.children.get(index) {
                self.expression(
                    Some(value),
                    environment,
                    region,
                    &format!("{path}:value:{index}"),
                    false,
                );
            }
            self.bind_pattern(
                pattern,
                expression.span,
                &format!("{path}:binding:{index}"),
                environment,
                region,
            );
            if let Some(guard) = expression.let_guards.get(index).and_then(Option::as_deref) {
                self.expression(
                    Some(guard),
                    environment,
                    region,
                    &format!("{path}:binding_guard:{index}"),
                    false,
                );
            }
        }
        if let Some(body) = expression.children.get(expression.patterns.len()) {
            self.expression(
                Some(body),
                environment,
                region,
                &format!("{path}:success"),
                true,
            );
        }
        self.clauses(&expression.clauses, &fallback_environment, path, "let_else");
    }

    fn comprehension(
        &mut self,
        expression: &SyntaxExprOutput,
        outer: &LexicalEnvironment,
        path: &str,
    ) {
        let mut environment = outer.clone();
        let mut region = self.region(path, environment.clone());
        for (index, pattern) in expression.patterns.iter().enumerate() {
            if let Some(source) = expression.children.get(index + 1) {
                self.expression(
                    Some(source),
                    &mut environment,
                    &mut region,
                    &format!("{path}:generator_source:{index}"),
                    false,
                );
            }
            self.bind_pattern(
                pattern,
                expression.span,
                &format!("{path}:generator:{index}"),
                &mut environment,
                &mut region,
            );
        }
        for (index, guard) in expression
            .children
            .iter()
            .skip(expression.patterns.len() + 1)
            .enumerate()
        {
            self.expression(
                Some(guard),
                &mut environment,
                &mut region,
                &format!("{path}:guard:{index}"),
                false,
            );
        }
        if let Some(yielded) = expression.children.first() {
            self.expression(
                Some(yielded),
                &mut environment,
                &mut region,
                &format!("{path}:yield"),
                true,
            );
        }
    }

    fn clauses(
        &mut self,
        clauses: &[SyntaxClauseOutput],
        outer: &LexicalEnvironment,
        path: &str,
        label: &str,
    ) {
        for (index, clause) in clauses.iter().enumerate() {
            let region_path = format!("{path}:{label}:{index}");
            let mut environment = outer.clone();
            let mut region = self.region(&region_path, environment.clone());
            for (pattern_index, pattern) in clause.patterns.iter().enumerate() {
                self.bind_pattern(
                    pattern,
                    clause.body.span,
                    &format!("{region_path}:pattern:{pattern_index}"),
                    &mut environment,
                    &mut region,
                );
            }
            self.expression(
                clause.guard.as_deref(),
                &mut environment,
                &mut region,
                &format!("{region_path}:guard"),
                false,
            );
            self.expression(
                Some(&clause.body),
                &mut environment,
                &mut region,
                &format!("{region_path}:body"),
                true,
            );
        }
    }

    fn nested_expression(
        &mut self,
        expression: &SyntaxExprOutput,
        outer: &LexicalEnvironment,
        path: &str,
    ) {
        let mut environment = outer.clone();
        let mut region = self.region(path, environment.clone());
        self.expression(Some(expression), &mut environment, &mut region, path, true);
    }

    fn html_node(&mut self, node: &SyntaxHtmlNodeOutput, outer: &LexicalEnvironment, path: &str) {
        match node {
            SyntaxHtmlNodeOutput::Expr { expr } => self.nested_expression(expr, outer, path),
            SyntaxHtmlNodeOutput::Text { .. } => {}
            SyntaxHtmlNodeOutput::Element { element } => {
                for (index, attr) in element.attrs.iter().enumerate() {
                    if let Some(crate::terlan_syntax::SyntaxHtmlAttrValueOutput::Expr { expr }) =
                        &attr.value
                    {
                        self.nested_expression(expr, outer, &format!("{path}:attr:{index}"));
                    }
                }
                for (index, child) in element.children.iter().enumerate() {
                    self.html_node(child, outer, &format!("{path}:child:{index}"));
                }
            }
            SyntaxHtmlNodeOutput::NamedSlot { slot } => {
                for (index, child) in slot.children.iter().enumerate() {
                    self.html_node(child, outer, &format!("{path}:slot:{index}"));
                }
            }
        }
    }

    fn bind_name(
        &mut self,
        name: &str,
        kind: CoreBindingKind,
        span: EbnfSourceSpan,
        path: &str,
        environment: &mut LexicalEnvironment,
        region: &mut Region,
    ) {
        if name.is_empty() || name == "_" {
            return;
        }
        if let Some(original) = region.local.get(name).copied() {
            let suggestion = non_colliding_name(name, &region.visible_names);
            region.visible_names.insert(suggestion.clone());
            self.collisions.push(BindingCollision {
                name: name.to_string(),
                original,
                region: region.id,
                path: path.to_string(),
                span,
                suggested_name: suggestion,
            });
            return;
        }
        let id = CoreBindingId(stable_nonzero_hash(&format!(
            "{}|{}|{}|{}",
            self.module, region.path, path, name
        )));
        region.local.insert(name.to_string(), id);
        region.visible_names.insert(name.to_string());
        environment.visible.insert(name.to_string(), id);
        self.bindings.push(CoreBindingIdentity {
            id,
            region: region.id,
            name: name.to_string(),
            kind,
            path: path.to_string(),
            span_start: span.start,
            span_end: span.end,
        });
    }

    fn reference(
        &mut self,
        name: &str,
        span: EbnfSourceSpan,
        path: &str,
        environment: &LexicalEnvironment,
    ) {
        let Some(binding) = environment.visible.get(name).copied() else {
            return;
        };
        self.references.push(CoreBindingReference {
            binding,
            name: name.to_string(),
            path: path.to_string(),
            span_start: span.start,
            span_end: span.end,
        });
    }

    fn region(&self, path: &str, environment: LexicalEnvironment) -> Region {
        Region {
            id: CoreBindingRegionId(stable_nonzero_hash(&format!(
                "{}|region|{path}",
                self.module
            ))),
            path: path.to_string(),
            local: BTreeMap::new(),
            visible_names: environment.visible.into_keys().collect(),
        }
    }
}

fn non_colliding_name(name: &str, visible: &BTreeSet<String>) -> String {
    for suffix in 2usize.. {
        let candidate = format!("{name}_{suffix}");
        if !visible.contains(&candidate) {
            return candidate;
        }
    }
    unreachable!("unbounded suffix search must find a name")
}

fn evidence_fingerprint(
    bindings: &[CoreBindingIdentity],
    references: &[CoreBindingReference],
) -> String {
    let mut text = String::new();
    for binding in bindings {
        text.push_str(&format!(
            "b:{:016x}:{:016x}:{}:{:?}:{}:{}:{}\n",
            binding.id.0,
            binding.region.0,
            binding.name,
            binding.kind,
            binding.path,
            binding.span_start,
            binding.span_end
        ));
    }
    for reference in references {
        text.push_str(&format!(
            "r:{:016x}:{}:{}:{}:{}\n",
            reference.binding.0,
            reference.name,
            reference.path,
            reference.span_start,
            reference.span_end
        ));
    }
    format!("{:016x}", stable_nonzero_hash(&text))
}

fn stable_nonzero_hash(text: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    if hash == 0 {
        1
    } else {
        hash
    }
}
