use terlan_typeck::CoreExpr;
use terlan_typeck::CoreExprSummary;
use terlan_typeck::CoreModule;
use terlan_typeck::CorePattern;

/// Diagnostic code emitted when unresolved constructor metadata reaches target
/// profile validation.
///
/// Inputs:
/// - No runtime input.
///
/// Output:
/// - Stable diagnostic code used by production validation and regression tests.
///
/// Transformation:
/// - Centralizes the unresolved-constructor target-profile violation label
///   without allocating or inspecting CoreIR.
const TARGET_PROFILE_UNRESOLVED_CONSTRUCTOR_CODE: &str = "target_profile_unresolved_constructor";

/// Formats the unresolved-constructor target-profile diagnostic message.
///
/// Inputs:
/// - `profile`: target profile being validated.
/// - `calls`: unresolved constructor-call candidate count.
/// - `chains`: unresolved constructor-chain candidate count.
/// - `patterns`: unresolved constructor-pattern candidate count.
///
/// Output:
/// - Stable diagnostic message with profile name and candidate counters.
///
/// Transformation:
/// - Converts profile and counter values into the user-facing backend-profile
///   constructor-resolution error message.
fn unresolved_constructor_message(
    profile: TargetProfile,
    calls: usize,
    chains: usize,
    patterns: usize,
) -> String {
    format!(
        "target `{}` requires constructor candidates to resolve before backend validation: calls={calls} chains={chains} patterns={patterns}",
        profile.as_str()
    )
}

mod profile;

pub(crate) use profile::TargetProfile;

mod std_runtime;

use std_runtime::{
    std_call_heads, std_module_import_violation, target_profile_supports_beam_agent_operation,
    target_profile_supports_beam_gen_server_operation,
    target_profile_supports_beam_native_bridge_operation,
    target_profile_supports_beam_supervisor_operation, target_profile_supports_beam_task_operation,
    target_profile_supports_task_operation, validate_core_imports,
    validate_std_runtime_operation_summary_support, validate_std_runtime_operation_support,
    StdCallHeads,
};

/// Structured target-profile violation with enough context for diagnostics.
#[derive(Debug)]
pub(crate) struct TargetProfileViolation {
    /// Stable violation code.
    pub(crate) code: &'static str,
    /// Human-readable explanation.
    pub(crate) message: String,
}

/// Command-level options for target-profile validation.
///
/// Inputs:
/// - Set by the command that invokes formal compilation.
///
/// Output:
/// - Validation switches that are independent of target capability identity.
///
/// Transformation:
/// - Lets commands that own filesystem asset resolution admit asset imports
///   while generic compile/check paths still reject them before backend
///   emission.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TargetProfileCheckOptions {
    pub(crate) allow_asset_imports: bool,
}

/// Formats an unsupported std-module target-profile diagnostic.
///
/// Inputs:
/// - `profile`: backend profile requested by a command.
/// - `context`: source, module, or function context used in diagnostics.
/// - `import_module`: fully qualified std module path from source or CoreIR.
///
/// Output:
/// - `Some(String)` when the module belongs to an unsupported std family.
/// - `None` when the profile admits the std module or the module is not
///   target-gated.
///
/// Transformation:
/// - Reuses the CoreIR target-profile import-family table for pre-lowering
///   command checks, keeping early build diagnostics aligned with formal
///   validation.
pub(crate) fn target_profile_std_module_import_error(
    profile: TargetProfile,
    context: &str,
    import_module: &str,
) -> Option<String> {
    std_module_import_violation(profile, context, import_module).map(|violation| violation.message)
}

impl TargetProfileViolation {
    /// Builds an unsupported-feature violation.
    ///
    /// Inputs:
    /// - `pattern`: violated feature category.
    /// - `profile`: profile that rejected the feature.
    /// - `context`: function, clause, or payload location.
    /// - `detail`: concrete rejected coverage or shape detail.
    ///
    /// Output:
    /// - Normalized `target_profile_unsupported` violation.
    ///
    /// Transformation:
    /// - Formats profile, category, detail, and context into a CLI-safe
    ///   diagnostic message.
    fn unsupported(pattern: &str, profile: TargetProfile, context: &str, detail: &str) -> Self {
        Self {
            code: "target_profile_unsupported",
            message: format!(
                "target `{}` does not support {} {} for {}",
                profile.as_str(),
                pattern,
                detail,
                context
            ),
        }
    }

    /// Builds a missing checked-preservation evidence violation.
    ///
    /// Inputs:
    /// - `profile`: profile that requires evidence.
    /// - `context`: function, clause, or payload location.
    /// - `detail`: missing evidence class.
    ///
    /// Output:
    /// - Normalized `target_profile_missing_evidence` violation.
    ///
    /// Transformation:
    /// - Formats profile, missing evidence detail, and context into a CLI-safe
    ///   diagnostic message.
    fn missing_evidence(profile: TargetProfile, context: &str, detail: &str) -> Self {
        Self {
            code: "target_profile_missing_evidence",
            message: format!(
                "target `{}` requires checked-preservation evidence {} for {}",
                profile.as_str(),
                detail,
                context
            ),
        }
    }
}

/// Validates a lowered `CoreModule` against a target profile.
///
/// Inputs:
/// - `module`: typed core module produced by formal lowering.
/// - `profile`: backend profile requested by the caller.
///
/// Output:
/// - A list of violations when module features exceed the selected profile.
///
/// Transformation:
/// - Rejects unresolved constructor candidates from CoreIR metadata, traverses
///   every function clause body, guard, and typed pattern summary, then
///   inspects typed core nodes for structural shape constraints.
#[cfg(test)]
fn target_profile_checks(
    module: &CoreModule,
    profile: TargetProfile,
) -> Vec<TargetProfileViolation> {
    target_profile_checks_with_options(module, profile, TargetProfileCheckOptions::default())
}

/// Validates a lowered `CoreModule` against a target profile with command-level
/// validation options.
///
/// Inputs:
/// - `module`: typed core module produced by formal lowering.
/// - `profile`: backend profile requested by the caller.
/// - `options`: command-owned target validation switches.
///
/// Output:
/// - A list of violations when module features exceed the selected profile and
///   command capabilities.
///
/// Transformation:
/// - Applies import-family checks that may depend on command-owned resolvers,
///   then runs the normal CoreIR shape, evidence, and constructor-resolution
///   profile validation.
pub(crate) fn target_profile_checks_with_options(
    module: &CoreModule,
    profile: TargetProfile,
    options: TargetProfileCheckOptions,
) -> Vec<TargetProfileViolation> {
    let mut violations = Vec::new();
    let std_call_heads = std_call_heads(module);

    validate_core_imports(profile, module, options, &mut violations);

    if module.metadata.unresolved_constructor_call_candidate_count
        + module.metadata.unresolved_constructor_chain_candidate_count
        + module
            .metadata
            .unresolved_constructor_pattern_candidate_count
        != 0
    {
        violations.push(TargetProfileViolation {
            code: TARGET_PROFILE_UNRESOLVED_CONSTRUCTOR_CODE,
            message: unresolved_constructor_message(
                profile,
                module.metadata.unresolved_constructor_call_candidate_count,
                module.metadata.unresolved_constructor_chain_candidate_count,
                module
                    .metadata
                    .unresolved_constructor_pattern_candidate_count,
            ),
        });
    }

    for function in &module.functions {
        for (clause_index, clause) in function.clauses.iter().enumerate() {
            let context = format!("function {}@{}", function.name, clause_index);
            for (pattern_index, pattern) in clause.core_patterns.iter().enumerate() {
                let location = format!("{context} pattern#{pattern_index}");
                match (pattern, clause.pattern_proof_coverage.get(pattern_index)) {
                    (Some(pattern), Some(coverage)) => {
                        if !profile.allows_pattern_coverage(*coverage) {
                            violations.push(TargetProfileViolation::unsupported(
                                "pattern coverage",
                                profile,
                                &location,
                                &format!("{coverage:?}"),
                            ));
                        }
                        if profile.requires_checked_preservation_evidence()
                            && clause
                                .pattern_checked_preservation_evidence
                                .get(pattern_index)
                                .is_none_or(Option::is_none)
                        {
                            violations.push(TargetProfileViolation::missing_evidence(
                                profile,
                                &location,
                                "for typed pattern payload",
                            ));
                        }
                        validate_core_pattern(profile, pattern, &location, &mut violations);
                    }
                    (None, Some(coverage)) => {
                        if !profile.allows_pattern_coverage(*coverage)
                            || !profile.allows_uncovered_pattern()
                        {
                            violations.push(TargetProfileViolation::unsupported(
                                "untyped pattern",
                                profile,
                                &location,
                                &format!("coverage={coverage:?}"),
                            ));
                        }
                    }
                    (_, None) => {}
                }
            }

            if let Some(coverage) = clause.pattern_proof_coverage.first() {
                if !profile.allows_pattern_coverage(*coverage) {
                    violations.push(TargetProfileViolation::unsupported(
                        "pattern coverage",
                        profile,
                        &context,
                        &format!("{coverage:?}"),
                    ));
                }
            }

            validate_core_expr_summary(
                profile,
                &std_call_heads,
                &context,
                "body",
                &clause.body,
                &mut violations,
            );
            if let Some(guard) = &clause.guard {
                validate_core_expr_summary(
                    profile,
                    &std_call_heads,
                    &context,
                    "guard",
                    guard,
                    &mut violations,
                );
            }
        }
    }

    violations
}

/// Validates one expression summary and its recursive child summaries.
///
/// Inputs:
/// - `profile`: backend-capability profile under validation.
/// - `std_call_heads`: module names proven to refer to target-gated std APIs.
/// - `function_scope`: enclosing function/clause label.
/// - `location`: summary location relative to the function scope.
/// - `summary`: expression summary produced by CoreIR lowering.
/// - `violations`: mutable output collection for profile violations.
///
/// Output:
/// - No direct return value; violations are appended in place.
///
/// Transformation:
/// - Checks expression proof coverage, runtime-boundary metadata,
///   checked-preservation evidence, typed payload availability, nested child
///   summaries, and typed Core expression shape.
fn validate_core_expr_summary(
    profile: TargetProfile,
    std_call_heads: &StdCallHeads,
    function_scope: &str,
    location: &str,
    summary: &CoreExprSummary,
    violations: &mut Vec<TargetProfileViolation>,
) {
    validate_std_runtime_operation_summary_support(
        profile,
        function_scope,
        location,
        summary,
        &std_call_heads.task,
        "task operation",
        "std.core.Task",
        target_profile_supports_task_operation,
        violations,
    );
    validate_std_runtime_operation_summary_support(
        profile,
        function_scope,
        location,
        summary,
        &std_call_heads.beam_agent,
        "BEAM Agent operation",
        "std.beam.Agent",
        target_profile_supports_beam_agent_operation,
        violations,
    );
    validate_std_runtime_operation_summary_support(
        profile,
        function_scope,
        location,
        summary,
        &std_call_heads.beam_gen_server,
        "BEAM GenServer operation",
        "std.beam.GenServer",
        target_profile_supports_beam_gen_server_operation,
        violations,
    );
    validate_std_runtime_operation_summary_support(
        profile,
        function_scope,
        location,
        summary,
        &std_call_heads.beam_native_bridge,
        "BEAM NativeBridge operation",
        "std.beam.NativeBridge",
        target_profile_supports_beam_native_bridge_operation,
        violations,
    );
    validate_std_runtime_operation_summary_support(
        profile,
        function_scope,
        location,
        summary,
        &std_call_heads.beam_supervisor,
        "BEAM Supervisor operation",
        "std.beam.Supervisor",
        target_profile_supports_beam_supervisor_operation,
        violations,
    );
    validate_std_runtime_operation_summary_support(
        profile,
        function_scope,
        location,
        summary,
        &std_call_heads.beam_task,
        "BEAM Task operation",
        "std.beam.Task",
        target_profile_supports_beam_task_operation,
        violations,
    );

    if !profile.allows_expr_coverage(summary.proof_coverage) {
        violations.push(TargetProfileViolation::unsupported(
            "expression coverage",
            profile,
            &format!("{function_scope} {location}"),
            &format!("{cover:?}", cover = summary.proof_coverage),
        ));
    }

    if summary.remote.is_some() && !profile.allows_runtime_boundary() {
        violations.push(TargetProfileViolation::unsupported(
            "runtime boundary",
            profile,
            &format!("{function_scope} {location}"),
            "remote call target",
        ));
    }

    if profile.requires_checked_preservation_evidence()
        && summary.core_expr.is_some()
        && summary.checked_preservation_evidence.is_none()
    {
        violations.push(TargetProfileViolation::missing_evidence(
            profile,
            &format!("{function_scope} {location}"),
            "for typed expression payload",
        ));
    }

    if !profile.allows_expr_summary_kind(summary) {
        violations.push(TargetProfileViolation::unsupported(
            "expression kind",
            profile,
            &format!("{function_scope} {location}"),
            &summary.kind,
        ));
    }

    if !profile.allows_expr_shape_if_present(summary) {
        violations.push(TargetProfileViolation::unsupported(
            "expression shape",
            profile,
            &format!("{function_scope} {location}"),
            "missing typed payload",
        ));
    }

    for (index, child) in summary.children.iter().enumerate() {
        validate_core_expr_summary(
            profile,
            std_call_heads,
            function_scope,
            &format!("{location} child[{index}]"),
            child,
            violations,
        );
    }

    if let Some(expr) = summary.core_expr.as_ref() {
        validate_core_expr(
            profile,
            std_call_heads,
            function_scope,
            location,
            expr,
            violations,
        );
    }
}

/// Validates one typed Core expression and recursively validates contained
/// expressions and patterns.
///
/// Inputs:
/// - `profile`: backend-capability profile under validation.
/// - `std_call_heads`: module names proven to refer to target-gated std APIs.
/// - `function_scope`: enclosing function/clause label.
/// - `location`: expression location relative to the function scope.
/// - `expr`: typed Core expression payload.
/// - `violations`: mutable output collection for profile violations.
///
/// Output:
/// - No direct return value; violations are appended in place.
///
/// Transformation:
/// - Checks the expression shape against the profile matrix and walks every
///   nested typed expression or pattern payload reachable from the node.
fn validate_core_expr(
    profile: TargetProfile,
    std_call_heads: &StdCallHeads,
    function_scope: &str,
    location: &str,
    expr: &CoreExpr,
    violations: &mut Vec<TargetProfileViolation>,
) {
    if let CoreExpr::Binary(value) = expr {
        if binary_expr_requires_segment_lowering(value) {
            violations.push(TargetProfileViolation::unsupported(
                "binary segment lowering",
                profile,
                &format!("{function_scope} {location}"),
                value,
            ));
        }
    }

    if !profile.allows_expr_shape(expr) {
        violations.push(TargetProfileViolation::unsupported(
            "typed expression shape",
            profile,
            &format!("{function_scope} {location}"),
            &format!("{expr:?}"),
        ));
    }

    match expr {
        CoreExpr::Int(_) => {}
        CoreExpr::Float(_) => {}
        CoreExpr::Binary(_) => {}
        CoreExpr::Atom(_) => {}
        CoreExpr::Var(_) => {}
        CoreExpr::Tuple(values) => {
            values.iter().for_each(|value| {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "tuple",
                    value,
                    violations,
                )
            });
        }
        CoreExpr::List(values) => {
            values.iter().for_each(|value| {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "list",
                    value,
                    violations,
                )
            });
        }
        CoreExpr::ListCons { head, tail } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "list head",
                head,
                violations,
            );
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "list tail",
                tail,
                violations,
            );
        }
        CoreExpr::FixedArray(values) => {
            values.iter().for_each(|value| {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "fixed array",
                    value,
                    violations,
                )
            });
        }
        CoreExpr::Index { base, index } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "index base",
                base,
                violations,
            );
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "index value",
                index,
                violations,
            );
        }
        CoreExpr::ListComprehension {
            expr,
            pattern,
            source,
            guard,
        } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "list comprehension expr",
                expr,
                violations,
            );
            validate_core_pattern(profile, pattern, "list comprehension pattern", violations);
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "list comprehension source",
                source,
                violations,
            );
            if let Some(guard) = guard {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "list comprehension guard",
                    guard,
                    violations,
                );
            }
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "let binding value",
                    &binding.value,
                    violations,
                );
            }
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "let body",
                body,
                violations,
            );
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "map field",
                    &field.value,
                    violations,
                );
            }
        }
        CoreExpr::RecordConstruct { fields, .. } => {
            for field in fields {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "record field",
                    &field.value,
                    violations,
                );
            }
        }
        CoreExpr::FieldAccess { base, .. } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "field access base",
                base,
                violations,
            );
        }
        CoreExpr::RecordAccess { base, .. } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "record access base",
                base,
                violations,
            );
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "record update base",
                base,
                violations,
            );
            for field in fields {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "record update field",
                    &field.value,
                    violations,
                );
            }
        }
        CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "template prop",
                    &field.value,
                    violations,
                );
            }
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            for arg in args {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "constructor chain arg",
                    arg,
                    violations,
                );
            }
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "constructor chain record",
                record,
                violations,
            );
        }
        CoreExpr::RemoteFunRef { .. } => {}
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } => {
            validate_std_runtime_operation_support(
                profile,
                function_scope,
                location,
                module,
                function,
                &std_call_heads.task,
                "task operation",
                "std.core.Task",
                target_profile_supports_task_operation,
                violations,
            );
            validate_std_runtime_operation_support(
                profile,
                function_scope,
                location,
                module,
                function,
                &std_call_heads.beam_agent,
                "BEAM Agent operation",
                "std.beam.Agent",
                target_profile_supports_beam_agent_operation,
                violations,
            );
            validate_std_runtime_operation_support(
                profile,
                function_scope,
                location,
                module,
                function,
                &std_call_heads.beam_gen_server,
                "BEAM GenServer operation",
                "std.beam.GenServer",
                target_profile_supports_beam_gen_server_operation,
                violations,
            );
            validate_std_runtime_operation_support(
                profile,
                function_scope,
                location,
                module,
                function,
                &std_call_heads.beam_native_bridge,
                "BEAM NativeBridge operation",
                "std.beam.NativeBridge",
                target_profile_supports_beam_native_bridge_operation,
                violations,
            );
            validate_std_runtime_operation_support(
                profile,
                function_scope,
                location,
                module,
                function,
                &std_call_heads.beam_supervisor,
                "BEAM Supervisor operation",
                "std.beam.Supervisor",
                target_profile_supports_beam_supervisor_operation,
                violations,
            );
            validate_std_runtime_operation_support(
                profile,
                function_scope,
                location,
                module,
                function,
                &std_call_heads.beam_task,
                "BEAM Task operation",
                "std.beam.Task",
                target_profile_supports_beam_task_operation,
                violations,
            );
            for arg in args {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "remote call arg",
                    arg,
                    violations,
                );
            }
        }
        CoreExpr::ConstructorCall { args, .. } => {
            for arg in args {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "constructor call arg",
                    arg,
                    violations,
                );
            }
        }
        CoreExpr::Call { args, .. } => {
            for arg in args {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "call arg",
                    arg,
                    violations,
                );
            }
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "mutable receiver call receiver",
                receiver,
                violations,
            );
            for arg in args {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "mutable receiver call arg",
                    arg,
                    violations,
                );
            }
        }
        CoreExpr::FunctionCall { callee, args } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "function call callee",
                callee,
                violations,
            );
            for arg in args {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "function call arg",
                    arg,
                    violations,
                );
            }
        }
        CoreExpr::Cast { expr, .. } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "cast expr",
                expr,
                violations,
            );
        }
        CoreExpr::Intrinsic(call) => {
            for arg in &call.args {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "intrinsic arg",
                    arg,
                    violations,
                );
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "case scrutinee",
                scrutinee,
                violations,
            );
            for clause in clauses {
                if let Some(guard) = &clause.guard {
                    validate_core_expr(
                        profile,
                        std_call_heads,
                        function_scope,
                        "case clause guard",
                        guard,
                        violations,
                    );
                }
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "case clause body",
                    &clause.body,
                    violations,
                );
                validate_core_pattern(profile, &clause.pattern, "case clause pattern", violations);
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "try body",
                body,
                violations,
            );
            for clause in of_clauses {
                if let Some(guard) = &clause.guard {
                    validate_core_expr(
                        profile,
                        std_call_heads,
                        function_scope,
                        "try of clause guard",
                        guard,
                        violations,
                    );
                }
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "try of clause body",
                    &clause.body,
                    violations,
                );
                validate_core_pattern(
                    profile,
                    &clause.pattern,
                    "try of clause pattern",
                    violations,
                );
            }
            for clause in catch_clauses {
                if let Some(guard) = &clause.guard {
                    validate_core_expr(
                        profile,
                        std_call_heads,
                        function_scope,
                        "try catch clause guard",
                        guard,
                        violations,
                    );
                }
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "try catch clause body",
                    &clause.body,
                    violations,
                );
                validate_core_pattern(
                    profile,
                    &clause.pattern,
                    "try catch clause pattern",
                    violations,
                );
            }
            if let Some(after_clause) = after_clause {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "try after trigger",
                    &after_clause.trigger,
                    violations,
                );
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "try after body",
                    &after_clause.body,
                    violations,
                );
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "if clause condition",
                    &clause.condition,
                    violations,
                );
                validate_core_expr(
                    profile,
                    std_call_heads,
                    function_scope,
                    "if clause body",
                    &clause.body,
                    violations,
                );
            }
        }
        CoreExpr::Lam { params, body, .. } => {
            for param in params {
                validate_core_pattern(profile, param, "function parameter", violations);
            }
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "lambda body",
                body,
                violations,
            );
        }
        CoreExpr::UnaryOp { operand, .. } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "unary operand",
                operand,
                violations,
            );
        }
        CoreExpr::BinaryOp { left, right, .. } => {
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "binary left",
                left,
                violations,
            );
            validate_core_expr(
                profile,
                std_call_heads,
                function_scope,
                "binary right",
                right,
                violations,
            );
        }
    }
}

/// Returns whether a binary literal still requires segment semantic lowering.
///
/// Inputs:
/// - `value`: source-preserved binary text from `CoreExpr::Binary`.
///
/// Output:
/// - `true` when the value uses structured `<<...>>` segment syntax beyond a
///   single string segment.
/// - `false` for plain string literals, empty binaries, and `<<"text">>`
///   string-only binary literals.
///
/// Transformation:
/// - Classifies source text without parsing segment semantics so target-profile
///   validation can reject deferred binary segment lowering before backend
///   emission.
fn binary_expr_requires_segment_lowering(value: &str) -> bool {
    let trimmed = value.trim();
    if !(trimmed.starts_with("<<") && trimmed.ends_with(">>")) {
        return false;
    }

    let inner = trimmed[2..trimmed.len() - 2].trim();
    if inner.is_empty() {
        return false;
    }

    !binary_expr_is_single_string_segment(inner)
}

/// Returns whether binary contents are exactly one string literal segment.
///
/// Inputs:
/// - `inner`: text between `<<` and `>>`.
///
/// Output:
/// - `true` when `inner` is one complete double-quoted string literal.
///
/// Transformation:
/// - Scans the string literal with escape handling and rejects trailing segment
///   modifiers, commas, or additional source text.
fn binary_expr_is_single_string_segment(inner: &str) -> bool {
    let mut chars = inner.char_indices();
    if chars.next().is_none_or(|(_, ch)| ch != '"') {
        return false;
    }

    let mut escaped = false;
    for (index, ch) in chars {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            return inner[index + ch.len_utf8()..].trim().is_empty();
        }
    }

    false
}

/// Validates one typed Core pattern and recursively validates contained
/// patterns.
///
/// Inputs:
/// - `profile`: backend-capability profile under validation.
/// - `pattern`: typed Core pattern payload.
/// - `location`: pattern location label for diagnostics.
/// - `violations`: mutable output collection for profile violations.
///
/// Output:
/// - No direct return value; violations are appended in place.
///
/// Transformation:
/// - Checks the pattern shape against the profile matrix and walks every nested
///   typed pattern payload reachable from the node.
fn validate_core_pattern(
    profile: TargetProfile,
    pattern: &CorePattern,
    location: &str,
    violations: &mut Vec<TargetProfileViolation>,
) {
    if !profile.allows_pattern_shape(pattern) {
        violations.push(TargetProfileViolation::unsupported(
            "pattern shape",
            profile,
            location,
            &format!("{pattern:?}"),
        ));
    }

    match pattern {
        CorePattern::Wildcard => {}
        CorePattern::Var(_) => {}
        CorePattern::Int(_) => {}
        CorePattern::Float(_) => {}
        CorePattern::Atom(_) => {}
        CorePattern::Tuple(values) => {
            for value in values {
                validate_core_pattern(profile, value, "tuple", violations);
            }
        }
        CorePattern::List(values) => {
            for value in values {
                validate_core_pattern(profile, value, "list", violations);
            }
        }
        CorePattern::ListCons { head, tail } => {
            validate_core_pattern(profile, head, "list head", violations);
            validate_core_pattern(profile, tail, "list tail", violations);
        }
        CorePattern::Map(fields) => {
            for field in fields {
                validate_core_pattern(profile, &field.value, "map field", violations);
            }
        }
        CorePattern::Record { fields, .. } => {
            for field in fields {
                validate_core_pattern(profile, &field.value, "record field", violations);
            }
        }
        CorePattern::Constructor { args, .. } => {
            for arg in args {
                validate_core_pattern(profile, arg, "constructor", violations);
            }
        }
    }
}

trait ProfileExprShapeExtensions {
    fn allows_expr_summary_kind(self, summary: &CoreExprSummary) -> bool;
    fn allows_expr_shape_if_present(self, summary: &CoreExprSummary) -> bool;
}

impl ProfileExprShapeExtensions for TargetProfile {
    /// Returns whether a syntax-summary expression kind belongs to this profile.
    ///
    /// Inputs:
    /// - `summary`: expression summary from typed lowering.
    ///
    /// Output:
    /// - `true` when the profile admits the source expression family.
    ///
    /// Transformation:
    /// - Gates dedicated function-value invocation syntax so earlier successor
    ///   profiles do not inherit `f.(args)` merely because it lowers to the
    ///   same backend call payload as local named calls.
    fn allows_expr_summary_kind(self, summary: &CoreExprSummary) -> bool {
        if summary.kind != "FunctionCall" {
            return true;
        }

        matches!(
            self,
            Self::Erlang
                | Self::JsShared
                | Self::JsBrowser
                | Self::JsWorker
                | Self::A016Erlang
                | Self::A017Erlang
                | Self::A018Erlang
                | Self::A019Erlang
                | Self::A020Erlang
                | Self::A021Erlang
        )
    }

    /// Returns whether a summary payload shape is acceptable when a typed payload
    /// exists for the current profile.
    ///
    /// Inputs:
    /// - `summary`: expression summary from typed lowering.
    ///
    /// Output:
    /// - `false` if typed payload is missing and profile requires typed payload
    ///   for the observed proof class.
    fn allows_expr_shape_if_present(self, summary: &CoreExprSummary) -> bool {
        if summary.core_expr.is_some() {
            return true;
        }

        match self {
            Self::Erlang | Self::JsShared | Self::JsBrowser | Self::JsWorker => true,
            Self::A0Erlang
            | Self::A01Erlang
            | Self::A02Erlang
            | Self::A03Erlang
            | Self::A04Erlang
            | Self::A05Erlang
            | Self::A06Erlang
            | Self::A07Erlang
            | Self::A08Erlang
            | Self::A09Erlang
            | Self::A010Erlang
            | Self::A011Erlang
            | Self::A012Erlang
            | Self::A013Erlang
            | Self::A014Erlang
            | Self::A015Erlang
            | Self::A016Erlang
            | Self::A017Erlang
            | Self::A018Erlang
            | Self::A019Erlang
            | Self::A020Erlang
            | Self::A021Erlang => summary.core_expr.is_some(),
            Self::CoreV0 => summary.core_expr.is_some(),
        }
    }
}

#[cfg(test)]
mod target_profile_test;
