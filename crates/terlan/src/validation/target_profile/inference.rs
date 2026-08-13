use crate::terlan_syntax::{
    ebnf::EbnfSourceSpan, SyntaxDeclarationPayload, SyntaxImportKind, SyntaxModuleOutput,
    SyntaxTypeOutput,
};

use super::{TargetFamily, TargetProfile};

/// Typed evidence used to infer the narrowest executable target profile.
///
/// Inputs:
/// - `imports`: fully qualified typed imports visible after resolution.
/// - `capabilities`: target-specific capability ids required by typed code.
/// - `annotations`: target-relevant annotation ids checked by the compiler.
///
/// Output:
/// - A compact evidence bag that target selection can evaluate before backend
///   validation.
///
/// Transformation:
/// - Keeps target inference derived from typed compiler facts instead of CLI
///   spelling or source-file location.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TargetInferenceInput {
    imports: Vec<TargetEvidence>,
    capabilities: Vec<String>,
    annotations: Vec<String>,
    abi_issues: Vec<TargetInferenceConflict>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// One target-relevant import value with optional source-span evidence.
struct TargetEvidence {
    value: String,
    span: Option<EbnfSourceSpan>,
}

impl TargetInferenceInput {
    /// Builds target inference evidence from typed compiler facts.
    ///
    /// Inputs:
    /// - `imports`: resolved module imports.
    /// - `capabilities`: runtime/native capabilities required by the program.
    /// - `annotations`: checked annotation ids that affect target selection.
    ///
    /// Output:
    /// - Target inference input with owned stable strings.
    ///
    /// Transformation:
    /// - Copies caller-provided evidence into a self-contained value so target
    ///   inference remains independent of parser and resolver lifetimes.
    #[cfg(test)]
    pub(crate) fn from_typed_evidence(
        imports: &[&str],
        capabilities: &[&str],
        annotations: &[&str],
    ) -> Self {
        Self {
            imports: imports
                .iter()
                .map(|value| TargetEvidence {
                    value: (*value).to_string(),
                    span: None,
                })
                .collect(),
            capabilities: capabilities
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            annotations: annotations
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            abi_issues: Vec::new(),
        }
    }

    /// Builds target inference evidence from parsed syntax modules.
    ///
    /// Inputs:
    /// - `modules`: parsed syntax outputs that participate in one command's
    ///   target decision.
    ///
    /// Output:
    /// - Target inference input containing module imports and browser asset
    ///   import capabilities.
    ///
    /// Transformation:
    /// - Converts structured import declarations into target evidence without
    ///   regexing source text or duplicating command-local source scanners.
    pub(crate) fn from_syntax_modules<'a>(
        modules: impl IntoIterator<Item = &'a SyntaxModuleOutput>,
    ) -> Self {
        let mut input = Self::default();
        for module in modules {
            input.append_syntax_module(module);
        }
        input
    }

    /// Appends one parsed syntax module to this target evidence bag.
    ///
    /// Inputs:
    /// - `module`: parsed syntax output.
    ///
    /// Output:
    /// - Mutates this input in place.
    ///
    /// Transformation:
    /// - Records normal module imports as import evidence and asset imports as
    ///   browser-runtime capability evidence.
    fn append_syntax_module(&mut self, module: &SyntaxModuleOutput) {
        let imports_wasm_abi = module.declarations.iter().any(|declaration| {
            matches!(
                &declaration.payload,
                SyntaxDeclarationPayload::Import {
                    import_kind: SyntaxImportKind::Module,
                    module_name,
                    ..
                } if is_wasm_abi_namespace(module_name)
            )
        });
        for declaration in &module.declarations {
            match &declaration.payload {
                SyntaxDeclarationPayload::Import {
                    import_kind,
                    module_name,
                    ..
                } => match import_kind {
                    SyntaxImportKind::Module => self.imports.push(TargetEvidence {
                        value: module_name.clone(),
                        span: Some(declaration.span),
                    }),
                    SyntaxImportKind::File | SyntaxImportKind::Css | SyntaxImportKind::Markdown => {
                        self.capabilities
                            .push("runtime.js.browser.asset".to_string());
                    }
                },
                SyntaxDeclarationPayload::Function {
                    name,
                    params,
                    return_type,
                    is_public: true,
                    ..
                } => {
                    for param in params {
                        self.validate_wasm_abi_slot(
                            imports_wasm_abi,
                            &format!("function `{name}` parameter `{}`", param.name),
                            &param.annotation,
                            false,
                        );
                    }
                    self.validate_wasm_abi_slot(
                        imports_wasm_abi,
                        &format!("function `{name}` return"),
                        return_type,
                        true,
                    );
                }
                _ => {}
            }
        }
    }

    /// Validates one parameter or result slot against WASM ABI import evidence.
    fn validate_wasm_abi_slot(
        &mut self,
        imports_wasm_abi: bool,
        slot: &str,
        ty: &SyntaxTypeOutput,
        is_return: bool,
    ) {
        let local_alias = is_local_wasm_scalar(&ty.text);
        let qualified_alias = is_qualified_wasm_scalar(&ty.text);
        if local_alias && !imports_wasm_abi {
            self.abi_issues.push(inference_conflict(
                "missing_abi_target",
                format!("{slot} uses `{}` without importing `std.wasm.Abi`", ty.text),
                Some(ty.span),
                vec![TargetFamily::Wasm],
            ));
            return;
        }
        if !imports_wasm_abi && !qualified_alias {
            return;
        }
        let portable = ty.text == "Int" || (is_return && ty.text == "Bool");
        if !portable && !local_alias && !qualified_alias {
            self.abi_issues.push(inference_conflict(
                "unsupported_abi_signature",
                format!(
                    "{slot} type `{}` is unsupported; expected I32, I64, F32, or F64",
                    ty.text
                ),
                Some(ty.span),
                vec![TargetFamily::Wasm],
            ));
        }
    }
}

/// Successful target inference result.
///
/// Inputs:
/// - Derived by evaluating typed target evidence.
///
/// Output:
/// - The inferred profile and source-facing reasons that justify the choice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TargetInference {
    pub(crate) profile: TargetProfile,
    pub(crate) reasons: Vec<String>,
}

/// Target inference conflict diagnostic.
///
/// Inputs:
/// - Conflicting typed evidence entries.
///
/// Output:
/// - Stable message and participating target families.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TargetInferenceConflict {
    pub(crate) code: &'static str,
    pub(crate) message: String,
    pub(crate) families: Vec<TargetFamily>,
    pub(crate) span: Option<EbnfSourceSpan>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EvidenceTarget {
    profile: TargetProfile,
    reason: String,
    span: Option<EbnfSourceSpan>,
}

/// Infers a target profile from typed imports, capabilities, and annotations.
///
/// Inputs:
/// - `input`: typed target evidence from resolver/typecheck summaries.
///
/// Output:
/// - `Ok(TargetInference)` with the narrowest supported target profile.
/// - `Err(TargetInferenceConflict)` when typed evidence requires incompatible
///   runtime families.
///
/// Transformation:
/// - Maps target-specific std namespaces and capabilities into profile
///   evidence, merges compatible evidence within a family, and defaults
///   target-neutral programs to the Terlan VM profile.
pub(crate) fn infer_target_profile_from_typed_evidence(
    input: &TargetInferenceInput,
) -> Result<TargetInference, TargetInferenceConflict> {
    if let Some(issue) = input.abi_issues.first() {
        return Err(issue.clone());
    }
    let mut selected: Option<EvidenceTarget> = None;
    let mut reasons = Vec::new();

    for import in &input.imports {
        if let Some(mut target) = infer_import_target(&import.value) {
            target.span = import.span;
            merge_evidence(&mut selected, &mut reasons, target)?;
        }
    }

    for capability in &input.capabilities {
        if let Some(target) = infer_capability_target(capability) {
            merge_evidence(&mut selected, &mut reasons, target)?;
        }
    }

    for annotation in &input.annotations {
        if let Some(target) = infer_annotation_target(annotation) {
            merge_evidence(&mut selected, &mut reasons, target)?;
        }
    }

    let profile = selected
        .as_ref()
        .map_or(TargetProfile::Vm, |target| target.profile);
    if reasons.is_empty() {
        reasons.push("no target-specific typed evidence; defaulting to vm".to_string());
    }

    Ok(TargetInference { profile, reasons })
}

/// Validates that an explicit profile override can satisfy inferred evidence.
///
/// Inputs:
/// - `inferred`: inferred target profile result.
/// - `explicit`: caller-selected override.
///
/// Output:
/// - `None` when the explicit profile can satisfy the inferred profile family.
/// - `Some(String)` when the override would hide typed target evidence.
///
/// Transformation:
/// - Lets CLI flags remain explicit overrides while preserving target inference
///   as the source of compatibility diagnostics.
pub(crate) fn explicit_target_profile_override_error(
    inferred: &TargetInference,
    explicit: TargetProfile,
) -> Option<String> {
    if inferred
        .reasons
        .iter()
        .any(|reason| reason == "no target-specific typed evidence; defaulting to vm")
    {
        return None;
    }

    let inferred_family = inferred.profile.family();
    let explicit_family = explicit.family();
    if inferred_family != explicit_family {
        return Some(format!(
            "explicit target `{}` conflicts with typed target evidence for `{}`",
            explicit.as_str(),
            inferred.profile.as_str()
        ));
    }

    if inferred.profile == TargetProfile::JsBrowser && explicit == TargetProfile::JsShared {
        return Some(
            "explicit target `js.shared` cannot satisfy browser-only typed evidence".to_string(),
        );
    }

    if inferred.profile == TargetProfile::JsWorker && explicit == TargetProfile::JsShared {
        return Some(
            "explicit target `js.shared` cannot satisfy worker-only typed evidence".to_string(),
        );
    }

    None
}

fn merge_evidence(
    selected: &mut Option<EvidenceTarget>,
    reasons: &mut Vec<String>,
    target: EvidenceTarget,
) -> Result<(), TargetInferenceConflict> {
    match selected {
        None => {
            reasons.push(target.reason.clone());
            *selected = Some(target);
            Ok(())
        }
        Some(current) if current.profile.family() == target.profile.family() => {
            let merged = narrowest_profile(current.profile, target.profile)?;
            current.profile = merged;
            reasons.push(target.reason);
            Ok(())
        }
        Some(current) => Err(inference_conflict(
            "target_ambiguous",
            format!(
                "typed target evidence requires both `{}` and `{}`",
                current.profile.as_str(),
                target.profile.as_str()
            ),
            target.span.or(current.span),
            vec![current.profile.family(), target.profile.family()],
        )),
    }
}

fn narrowest_profile(
    current: TargetProfile,
    candidate: TargetProfile,
) -> Result<TargetProfile, TargetInferenceConflict> {
    if current == candidate {
        return Ok(current);
    }

    match (current, candidate) {
        (TargetProfile::JsShared, TargetProfile::JsBrowser)
        | (TargetProfile::JsBrowser, TargetProfile::JsShared) => Ok(TargetProfile::JsBrowser),
        (TargetProfile::JsShared, TargetProfile::JsWorker)
        | (TargetProfile::JsWorker, TargetProfile::JsShared) => Ok(TargetProfile::JsWorker),
        (TargetProfile::JsBrowser, TargetProfile::JsWorker)
        | (TargetProfile::JsWorker, TargetProfile::JsBrowser) => Err(inference_conflict(
            "target_ambiguous",
            "typed target evidence requires both browser and worker JavaScript profiles"
                .to_string(),
            None,
            vec![TargetFamily::Js],
        )),
        _ => Ok(current),
    }
}

fn infer_import_target(import: &str) -> Option<EvidenceTarget> {
    if import == "std.js.Dom" || import.starts_with("std.js.Dom.") {
        return Some(evidence(
            TargetProfile::JsBrowser,
            format!("import `{import}` requires js.browser"),
        ));
    }

    if import == "std.js.Worker" || import.starts_with("std.js.Worker.") {
        return Some(evidence(
            TargetProfile::JsWorker,
            format!("import `{import}` requires js.worker"),
        ));
    }

    if import == "std.js" || import.starts_with("std.js.") {
        return Some(evidence(
            TargetProfile::JsShared,
            format!("import `{import}` requires js.shared"),
        ));
    }

    if import == "std.wasm" || import.starts_with("std.wasm.") {
        return Some(evidence(
            TargetProfile::WasmCore,
            format!("import `{import}` requires wasm.core"),
        ));
    }

    if import == "std.vm"
        || import.starts_with("std.vm.")
        || import == "std.native"
        || import.starts_with("std.native.")
        || import == "std.db"
        || import.starts_with("std.db.")
    {
        return Some(evidence(
            TargetProfile::Vm,
            format!("import `{import}` requires vm"),
        ));
    }

    None
}

fn infer_capability_target(capability: &str) -> Option<EvidenceTarget> {
    if capability.starts_with("runtime.js.dom") || capability.starts_with("runtime.js.browser") {
        return Some(evidence(
            TargetProfile::JsBrowser,
            format!("capability `{capability}` requires js.browser"),
        ));
    }

    if capability.starts_with("runtime.js.worker") {
        return Some(evidence(
            TargetProfile::JsWorker,
            format!("capability `{capability}` requires js.worker"),
        ));
    }

    if capability.starts_with("runtime.js") {
        return Some(evidence(
            TargetProfile::JsShared,
            format!("capability `{capability}` requires js.shared"),
        ));
    }

    if capability.starts_with("runtime.wasm") {
        return Some(evidence(
            TargetProfile::WasmCore,
            format!("capability `{capability}` requires wasm.core"),
        ));
    }

    if capability.starts_with("runtime.vm")
        || capability.starts_with("runtime.native")
        || capability.starts_with("runtime.http")
        || capability.starts_with("runtime.db")
    {
        return Some(evidence(
            TargetProfile::Vm,
            format!("capability `{capability}` requires vm"),
        ));
    }

    None
}

fn infer_annotation_target(annotation: &str) -> Option<EvidenceTarget> {
    if annotation.starts_with("target.js.browser") {
        return Some(evidence(
            TargetProfile::JsBrowser,
            format!("annotation `{annotation}` requires js.browser"),
        ));
    }

    if annotation.starts_with("target.js.worker") {
        return Some(evidence(
            TargetProfile::JsWorker,
            format!("annotation `{annotation}` requires js.worker"),
        ));
    }

    if annotation.starts_with("target.js") {
        return Some(evidence(
            TargetProfile::JsShared,
            format!("annotation `{annotation}` requires js.shared"),
        ));
    }

    if annotation.starts_with("target.wasm") {
        return Some(evidence(
            TargetProfile::WasmCore,
            format!("annotation `{annotation}` requires wasm.core"),
        ));
    }

    if annotation.starts_with("target.vm") {
        return Some(evidence(
            TargetProfile::Vm,
            format!("annotation `{annotation}` requires vm"),
        ));
    }

    None
}

fn evidence(profile: TargetProfile, reason: String) -> EvidenceTarget {
    EvidenceTarget {
        profile,
        reason,
        span: None,
    }
}

/// Builds one stable target-inference conflict with optional source context.
fn inference_conflict(
    code: &'static str,
    detail: String,
    span: Option<EbnfSourceSpan>,
    families: Vec<TargetFamily>,
) -> TargetInferenceConflict {
    let message = match span {
        Some(span) => format!("{code}: {detail} at span {}..{}", span.start, span.end),
        None => format!("{code}: {detail}"),
    };
    TargetInferenceConflict {
        code,
        message,
        families,
        span,
    }
}

/// Reports whether an import belongs to the standard WASM ABI namespace.
fn is_wasm_abi_namespace(module: &str) -> bool {
    module == "std.wasm.Abi" || module.starts_with("std.wasm.Abi.")
}

/// Reports whether a type uses a locally imported WASM scalar name.
fn is_local_wasm_scalar(ty: &str) -> bool {
    matches!(ty, "I32" | "I64" | "F32" | "F64")
}

/// Reports whether a type uses a fully qualified WASM scalar name.
fn is_qualified_wasm_scalar(ty: &str) -> bool {
    ty.strip_prefix("std.wasm.Abi.")
        .is_some_and(is_local_wasm_scalar)
}
