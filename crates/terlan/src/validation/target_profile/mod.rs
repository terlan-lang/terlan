use crate::terlan_typeck::CoreModule;

#[cfg(test)]
use crate::terlan_typeck::{CoreExpr, CoreExprSummary, CorePattern};

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

pub(crate) use profile::{TargetFamily, TargetProfile};

mod core_traversal;
mod std_runtime;
mod summary_shape;

use core_traversal::{validate_core_expr_summary, validate_core_pattern};
use std_runtime::{std_call_heads, std_module_import_violation, validate_core_imports};

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
    pub(crate) allow_rust_backed_std_modules: bool,
}

/// Formats an unsupported std-module target-profile diagnostic using
/// command-owned validation options.
///
/// Inputs:
/// - `profile`: backend profile requested by a command.
/// - `options`: command-owned target validation switches.
/// - `context`: source, module, or function context used in diagnostics.
/// - `import_module`: fully qualified std module path from source or CoreIR.
///
/// Output:
/// - `Some(String)` when the module belongs to an unsupported std family.
/// - `None` when the profile and command options admit the std module.
///
/// Transformation:
/// - Lets build/test commands that own SafeNative packaging admit portable
///   Rust-backed std modules while pure validation and incompatible targets
///   continue producing the same family diagnostic.
pub(crate) fn target_profile_std_module_import_error_with_options(
    profile: TargetProfile,
    options: TargetProfileCheckOptions,
    context: &str,
    import_module: &str,
) -> Option<String> {
    std_module_import_violation(profile, options, context, import_module)
        .map(|violation| violation.message)
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
    pub(super) fn unsupported(
        pattern: &str,
        profile: TargetProfile,
        context: &str,
        detail: &str,
    ) -> Self {
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
    pub(super) fn missing_evidence(profile: TargetProfile, context: &str, detail: &str) -> Self {
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

#[cfg(test)]
mod target_profile_test;
