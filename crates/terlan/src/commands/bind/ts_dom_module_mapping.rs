use super::ts_parser_adapter::{
    TsCallKind, TsDeclaration, TsDeclarationFile, TsInterfaceDeclaration, TsInterfaceMember,
    TsMethodDeclaration, TsParameterDeclaration, TsPropertyDeclaration, TsTypeAliasDeclaration,
    TsUnsupportedDeclaration, TsUnsupportedMember,
};
use super::ts_type_mapping::{
    map_ts_type_to_terlan, TsTupleElement, TsTypeMapping, TsTypeRef, TsTypeSkip,
};
use crate::terlan_hir::source_name_to_terlan_identifier;
use std::collections::{BTreeMap, BTreeSet};

/// DOM binding module mapping result.
///
/// Inputs:
/// - Produced from the neutral TypeScript declaration file model.
///
/// Output:
/// - Planned DOM modules plus stable skipped-declaration diagnostics.
///
/// Transformation:
/// - Converts TypeScript interfaces into deterministic `std.js.Dom.*` module
///   plans without writing generated files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DomModuleMapping {
    pub(super) modules: Vec<DomModulePlan>,
    pub(super) skipped: Vec<DomSkippedDeclaration>,
}

/// Planned generated DOM module.
///
/// Inputs:
/// - One supported TypeScript interface declaration.
///
/// Output:
/// - Module path, generated output paths, generated test path, default type
///   name, and mapped members.
///
/// Transformation:
/// - Applies Terlan module-layout conventions before concrete file emission
///   exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DomModulePlan {
    pub(super) constructible: bool,
    pub(super) module_path: String,
    pub(super) source_interface: String,
    pub(super) doc: Option<String>,
    pub(super) type_name: String,
    pub(super) type_params: Vec<String>,
    pub(super) alias_target: Option<String>,
    pub(super) source_path: String,
    pub(super) interface_path: String,
    pub(super) summary_path: String,
    pub(super) test_path: String,
    pub(super) members: Vec<DomMemberPlan>,
}

impl DomModulePlan {
    /// Required method-only interfaces can supply callbacks on an application object.
    pub(super) fn callback_members(&self) -> Option<Vec<&DomMethodPlan>> {
        if self.alias_target.is_some() || !self.type_params.is_empty() || self.members.is_empty() {
            return None;
        }
        let mut names = BTreeSet::new();
        self.members
            .iter()
            .map(|member| match member {
                DomMemberPlan::Method(method)
                    if !method.optional
                        && !method.partial
                        && method.call_kind == TsCallKind::Method
                        && method.type_params.is_empty()
                        && method.terlan_name != "with_members"
                        && names.insert(method.js_name.as_str())
                        && method
                            .params
                            .iter()
                            .all(|parameter| !parameter.optional && !parameter.rest) =>
                {
                    Some(method)
                }
                _ => None,
            })
            .collect()
    }
}

/// Planned generated DOM module member.
///
/// Inputs:
/// - Supported TypeScript interface properties and methods.
///
/// Output:
/// - Property or method plan with Terlan and JavaScript names.
///
/// Transformation:
/// - Preserves JavaScript source names while deriving deterministic Terlan
///   `snake_case` names for the generated API surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DomMemberPlan {
    Property(DomPropertyPlan),
    Method(DomMethodPlan),
}

/// Planned generated DOM property.
///
/// Inputs:
/// - One TypeScript interface property.
///
/// Output:
/// - Source JS name, Terlan field/method name, readonly/optional metadata, and
///   mapped Terlan type.
///
/// Transformation:
/// - Converts the TypeScript type through the T0.3 mapper before generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DomPropertyPlan {
    pub(super) write_supported: bool,
    pub(super) js_name: String,
    pub(super) terlan_name: String,
    pub(super) doc: Option<String>,
    pub(super) readonly: bool,
    pub(super) optional: bool,
    pub(super) undefined_only: bool,
    pub(super) terlan_type: String,
}

/// Planned generated DOM method.
///
/// Inputs:
/// - One TypeScript interface method.
///
/// Output:
/// - Source JS name, Terlan method name, mapped parameters, and mapped return
///   type.
///
/// Transformation:
/// - Converts all TypeScript types through the T0.3 mapper before generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DomMethodPlan {
    pub(super) partial: bool,
    pub(super) call_kind: TsCallKind,
    pub(super) type_params: Vec<String>,
    pub(super) type_constraints: Vec<Option<String>>,
    pub(super) callable_bounds: Vec<String>,
    pub(super) js_name: String,
    pub(super) terlan_name: String,
    pub(super) doc: Option<String>,
    pub(super) optional: bool,
    pub(super) params: Vec<DomParamPlan>,
    pub(super) return_type: String,
    pub(super) undefined_only: bool,
}

impl DomMethodPlan {
    pub(super) fn has_receiver(&self) -> bool {
        self.call_kind != TsCallKind::TuplePack
    }
    pub(super) fn callable_constraint_list(&self) -> String {
        if self.callable_bounds.is_empty() {
            return String::new();
        }
        let constraints = self
            .callable_bounds
            .iter()
            .map(|ty| format!("std.js.Callable.Callable[{ty}]"))
            .collect::<Vec<_>>()
            .join(", ");
        format!(" [{constraints}]")
    }
    pub(super) fn generic_declarations(&self) -> impl Iterator<Item = String> + '_ {
        self.type_params
            .iter()
            .zip(&self.type_constraints)
            .map(|(name, constraint)| {
                constraint
                    .as_ref()
                    .map_or_else(|| name.clone(), |shape| format!("{name} => {shape}"))
            })
    }
}

/// Planned generated DOM method parameter.
///
/// Inputs:
/// - One TypeScript method parameter.
///
/// Output:
/// - Source JS parameter name, Terlan parameter name, optional metadata, and
///   mapped Terlan type.
///
/// Transformation:
/// - Normalizes names with the same rule as generated properties/methods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DomParamPlan {
    pub(super) js_name: String,
    pub(super) terlan_name: String,
    pub(super) optional: bool,
    pub(super) rest: bool,
    pub(super) terlan_type: String,
}

/// Stable skipped DOM declaration diagnostic.
///
/// Inputs:
/// - Produced when TypeScript-to-Terlan mapping cannot safely emit a member.
///
/// Output:
/// - Source declaration path, stable reason code, and source type label.
///
/// Transformation:
/// - Gives future generation manifests deterministic skip entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DomSkippedDeclaration {
    pub(super) source: String,
    pub(super) reason: &'static str,
    pub(super) detail: String,
}

/// Converts a TypeScript type declaration name into canonical Terlan casing.
pub(super) fn terlan_type_name(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {
            format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
        }
        _ => name.to_string(),
    }
}

/// Maps parsed TypeScript declarations into DOM module plans.
///
/// Inputs:
/// - `declarations`: neutral TypeScript declaration file from the Oxc adapter.
///
/// Output:
/// - `DomModuleMapping` containing generated module plans and skipped members.
///
/// Transformation:
/// - Creates one `std.js.Dom.<Interface>` module per TypeScript interface and
///   maps supported members through the T0.3 type mapper.
pub(super) fn map_ts_declarations_to_dom_modules(
    declarations: &TsDeclarationFile,
) -> DomModuleMapping {
    map_declarations(declarations, false)
}

pub(super) fn map_declarations(
    declarations: &TsDeclarationFile,
    direct_calls: bool,
) -> DomModuleMapping {
    let mut modules = Vec::new();
    let mut skipped = Vec::new();

    for declaration in &declarations.declarations {
        match declaration {
            TsDeclaration::Interface(interface) => {
                if let Some(module) = map_interface_to_module(interface, direct_calls, &mut skipped)
                {
                    modules.push(module);
                }
            }
            TsDeclaration::TypeAlias(alias) => {
                if let Some(module) = map_type_alias_to_module(alias, direct_calls, &mut skipped) {
                    modules.push(module);
                }
            }
            TsDeclaration::Unsupported(unsupported) => {
                skipped.push(map_unsupported_declaration(unsupported));
            }
        }
    }

    let modules = merge_duplicate_modules(modules, &mut skipped);

    DomModuleMapping { modules, skipped }
}

/// Merges TypeScript declaration-merged modules into one Terlan module plan.
///
/// Inputs:
/// - `modules`: module plans produced in declaration order.
/// - `skipped`: shared skipped declaration accumulator.
///
/// Output:
/// - One module plan per generated Terlan module path.
///
/// Transformation:
/// - Models TypeScript interface declaration merging at the binding boundary
///   while refusing duplicate Terlan member shapes that would render ambiguous
///   functions.
fn merge_duplicate_modules(
    modules: Vec<DomModulePlan>,
    skipped: &mut Vec<DomSkippedDeclaration>,
) -> Vec<DomModulePlan> {
    let mut merged = Vec::new();
    let mut indexes = BTreeMap::new();
    for module in modules {
        let Some(index) = indexes.get(&module.module_path).copied() else {
            indexes.insert(module.module_path.clone(), merged.len());
            merged.push(module);
            continue;
        };
        let existing: &mut DomModulePlan = &mut merged[index];
        if !compatible_module_merge(existing, &module) {
            skipped.push(DomSkippedDeclaration {
                source: module.module_path.clone(),
                reason: "ts_bindgen.duplicate_module_conflict",
                detail:
                    "duplicate TypeScript declarations mapped to incompatible Terlan module outputs"
                        .to_string(),
            });
            continue;
        }
        if existing.doc.is_none() {
            existing.doc = module.doc.clone();
        }
        existing.constructible &= module.constructible;
        let skipped_before = skipped.len();
        merge_module_members(existing, module.members, skipped);
        existing.constructible &= skipped_before == skipped.len();
    }
    merged
}

/// Returns whether two generated module plans can be declaration-merged.
fn compatible_module_merge(left: &DomModulePlan, right: &DomModulePlan) -> bool {
    left.source_path == right.source_path
        && left.summary_path == right.summary_path
        && left.test_path == right.test_path
        && left.type_name == right.type_name
        && left.type_params == right.type_params
        && left.alias_target == right.alias_target
}

/// Appends non-conflicting members from a duplicate module declaration.
fn merge_module_members(
    target: &mut DomModulePlan,
    members: Vec<DomMemberPlan>,
    skipped: &mut Vec<DomSkippedDeclaration>,
) {
    let mut keys = target
        .members
        .iter()
        .map(member_key)
        .collect::<BTreeSet<_>>();
    for member in members {
        let key = member_key(&member);
        if keys.insert(key.clone()) {
            target.members.push(member);
            continue;
        }
        skipped.push(DomSkippedDeclaration {
            source: format!(
                "{}.{}",
                target.source_interface,
                member_source_name(&member)
            ),
            reason: "ts_bindgen.duplicate_member_shape",
            detail: format!("duplicate TypeScript member maps to existing Terlan member `{key}`"),
        });
    }
}

/// Returns a Terlan member identity that must be unique inside one module.
fn member_key(member: &DomMemberPlan) -> String {
    match member {
        DomMemberPlan::Property(property) => format!("property:{}", property.terlan_name),
        DomMemberPlan::Method(method) => {
            format!("method:{}/{}", method.terlan_name, method.params.len())
        }
    }
}

/// Returns the JavaScript source member name for diagnostics.
fn member_source_name(member: &DomMemberPlan) -> &str {
    match member {
        DomMemberPlan::Property(property) => &property.js_name,
        DomMemberPlan::Method(method) => &method.js_name,
    }
}

/// Converts a parser-level unsupported top-level declaration into a DOM skip row.
///
/// Inputs:
/// - `unsupported`: parser adapter skip metadata.
///
/// Output:
/// - DOM-level skipped declaration row.
///
/// Transformation:
/// - Preserves manifest namespace and stable reason text for declarations that
///   are intentionally absent from generated std.js output.
fn map_unsupported_declaration(unsupported: &TsUnsupportedDeclaration) -> DomSkippedDeclaration {
    DomSkippedDeclaration {
        source: unsupported.source.clone(),
        reason: unsupported.reason,
        detail: unsupported.detail.clone(),
    }
}

/// Maps one TypeScript interface into a DOM module plan.
///
/// Inputs:
/// - `interface`: neutral TypeScript interface declaration.
/// - `skipped`: shared skipped-declaration accumulator.
///
/// Output:
/// - `DomModulePlan` for the interface.
///
/// Transformation:
/// - Derives module/output paths from the interface name and maps each
///   supported member independently so one skipped member does not discard the
///   entire module.
fn map_interface_to_module(
    interface: &TsInterfaceDeclaration,
    direct_calls: bool,
    skipped: &mut Vec<DomSkippedDeclaration>,
) -> Option<DomModulePlan> {
    if !direct_calls {
        if let Some(error) = &interface.callable_alias {
            skipped.push(DomSkippedDeclaration {
                source: format!("{}.{}", interface.namespace, interface.name),
                reason: error.reason,
                detail: error.message.clone(),
            });
            return None;
        }
    }
    if reserved_bridge_interface_reason(&interface.name).is_some() {
        skipped.push(DomSkippedDeclaration {
            source: interface.name.clone(),
            reason: "ts_bindgen.reserved_bridge_interface",
            detail: "interface is owned by a hand-authored Terlan JavaScript bridge wrapper"
                .to_string(),
        });
        return None;
    }

    let namespace = binding_namespace(&interface.namespace);
    let type_name = terlan_type_name(&interface.name);
    let this_type = if interface.type_params.is_empty() {
        TsTypeRef::Named(type_name.clone())
    } else {
        TsTypeRef::Generic {
            name: type_name.clone(),
            args: interface
                .type_params
                .iter()
                .cloned()
                .map(TsTypeRef::Named)
                .collect(),
        }
    };
    let skipped_before = skipped.len();
    let mut members = Vec::new();
    let mut unsupported_member_sources = BTreeMap::new();
    for member in &interface.members {
        if !direct_calls {
            if let Some(unsupported) = legacy_method_skip(member) {
                let source = unique_unsupported_member_source(
                    &interface.name,
                    &unsupported,
                    &mut unsupported_member_sources,
                );
                skipped.push(map_unsupported_member(source, &unsupported));
                continue;
            }
        }
        match member {
            TsInterfaceMember::Property(property) => {
                if let Some(mut plan) = map_property(
                    &interface.name,
                    &interface.type_params,
                    &this_type,
                    property,
                    skipped,
                ) {
                    plan.write_supported &= interface.plain_record;
                    members.push(DomMemberPlan::Property(plan));
                }
            }
            TsInterfaceMember::Method(method) => {
                if let Some(plan) =
                    map_method(&interface.name, &this_type, method, direct_calls, skipped)
                {
                    members.push(DomMemberPlan::Method(plan));
                }
            }
            TsInterfaceMember::Unsupported(unsupported) => {
                let source = unique_unsupported_member_source(
                    &interface.name,
                    unsupported,
                    &mut unsupported_member_sources,
                );
                skipped.push(map_unsupported_member(source, unsupported));
            }
        }
    }

    let module_path = format!("{namespace}.{type_name}");
    let source_dir = namespace_to_source_dir(namespace);
    Some(DomModulePlan {
        constructible: interface.plain_record && skipped_before == skipped.len() && members.iter().all(|member| matches!(member, DomMemberPlan::Property(property) if property.write_supported)),
        module_path: module_path.clone(),
        source_interface: interface.name.clone(),
        doc: interface.doc.clone(),
        type_name: type_name.clone(),
        type_params: interface.type_params.clone(),
        alias_target: None,
        source_path: format!("{source_dir}/{type_name}.terl"),
        interface_path: format!("{source_dir}/{type_name}.terli"),
        summary_path: format!("std/summaries/{module_path}.typi"),
        test_path: format!("{source_dir}/{type_name}Test.terl"),
        members,
    })
}

fn legacy_method_skip(member: &TsInterfaceMember) -> Option<TsUnsupportedMember> {
    let reason = "ts_bindgen.unsupported_generic_constraint";
    let source = match member {
        TsInterfaceMember::Method(method) if method.call_kind != TsCallKind::Method => &method.name,
        TsInterfaceMember::Unsupported(member)
            if matches!(
                member.source.as_str(),
                "call_signature" | "construct_signature"
            ) =>
        {
            &member.source
        }
        TsInterfaceMember::Method(method)
            if method.type_constraints.iter().any(Option::is_some) =>
        {
            &method.name
        }
        TsInterfaceMember::Unsupported(member) if member.reason == reason => &member.source,
        _ => return None,
    };
    let (reason, detail) = if matches!(source.as_str(), "call_signature" | "construct_signature") {
        (
            "ts_bindgen.unsupported_interface_signature",
            "only named properties and methods are supported",
        )
    } else {
        (reason, "generic method constraints must not be erased")
    };
    Some(TsUnsupportedMember {
        source: source.into(),
        reason,
        detail: detail.into(),
    })
}

/// Maps one TypeScript type alias into a generated Terlan module plan.
///
/// Inputs:
/// - `alias`: neutral TypeScript type alias declaration.
/// - `skipped`: shared skipped-declaration accumulator.
///
/// Output:
/// - `DomModulePlan` for aliases whose target type maps conservatively.
/// - `None` with a skip diagnostic when the target type is unsupported.
///
/// Transformation:
/// - Emits one module per namespace alias, using `pub type Name = Target.` as
///   the generated Terlan surface.
fn map_type_alias_to_module(
    alias: &TsTypeAliasDeclaration,
    declaration_graph: bool,
    skipped: &mut Vec<DomSkippedDeclaration>,
) -> Option<DomModulePlan> {
    if alias.imported {
        skipped.push(DomSkippedDeclaration {
            source: qualified_alias_source(alias),
            reason: "ts_bindgen.unresolved_type_reference",
            detail: "imported alias must be resolved through the declaration graph".into(),
        });
        return None;
    }
    let namespace = binding_namespace(&alias.namespace);
    let type_name = terlan_type_name(&alias.name);
    let module_path = format!("{namespace}.{type_name}");
    let source_dir = namespace_to_source_dir(namespace);
    let source = qualified_alias_source(alias);
    let (alias_target, members) = match &alias.ty {
        TsTypeRef::Tuple(elements)
            if declaration_graph
                && (elements.is_empty() || elements.iter().any(|element| element.rest)) =>
        {
            (
                None,
                vec![DomMemberPlan::Method(map_tuple_pack(
                    alias,
                    elements,
                    &module_path,
                    skipped,
                )?)],
            )
        }
        _ => {
            let mapped = map_ts_type_to_terlan(&alias.ty);
            if let Some(skip) = mapped.skipped.first() {
                skipped.push(DomSkippedDeclaration {
                    source,
                    reason: skip.reason,
                    detail: format!("unsupported type alias target {}", skip.source),
                });
                return None;
            }
            (Some(mapped.terlan_type?), Vec::new())
        }
    };
    Some(DomModulePlan {
        constructible: false,
        module_path: module_path.clone(),
        source_interface: alias.name.clone(),
        doc: alias.doc.clone(),
        type_name: type_name.clone(),
        type_params: alias.type_params.clone(),
        alias_target,
        source_path: format!("{source_dir}/{type_name}.terl"),
        interface_path: format!("{source_dir}/{type_name}.terli"),
        summary_path: format!("std/summaries/{module_path}.typi"),
        test_path: format!("{source_dir}/{type_name}Test.terl"),
        members,
    })
}

fn map_tuple_pack(
    alias: &TsTypeAliasDeclaration,
    elements: &[TsTupleElement],
    module_path: &str,
    skipped: &mut Vec<DomSkippedDeclaration>,
) -> Option<DomMethodPlan> {
    let failure = |skipped: &mut Vec<DomSkippedDeclaration>, detail: &str| {
        skipped.push(DomSkippedDeclaration {
            source: qualified_alias_source(alias),
            reason: "ts_bindgen.unsupported_tuple_shape",
            detail: detail.into(),
        });
        None
    };
    if alias
        .type_constraints
        .iter()
        .flatten()
        .any(|constraint| *constraint != TsTypeRef::CallableBound)
    {
        return failure(
            skipped,
            "tuple pack requires enforcement of the alias type constraints",
        );
    }
    if elements.iter().any(|element| element.optional)
        || elements.iter().filter(|element| element.rest).count() > 1
    {
        return failure(
            skipped,
            "tuple pack requires fixed slots and at most one homogeneous rest slot",
        );
    }
    let mut names = BTreeSet::new();
    let mut params = Vec::new();
    for (index, element) in elements.iter().enumerate() {
        if element.rest
            && !matches!(&element.ty, TsTypeRef::Array(_))
            && !matches!(&element.ty, TsTypeRef::Generic { name, args } if matches!(name.as_str(), "Array" | "ReadonlyArray") && args.len() == 1)
        {
            return failure(
                skipped,
                "tuple rest slot requires a native array element type",
            );
        }
        let terlan_type = mapped_type_or_skip(
            format!("{} slot {index}", qualified_alias_source(alias)),
            map_ts_type_to_terlan(&element.ty),
            skipped,
        )?;
        if terlan_type.contains("Option[") {
            return failure(
                skipped,
                "tuple pack requires direct JavaScript values in its slots",
            );
        }
        let name = element
            .label
            .clone()
            .unwrap_or_else(|| format!("item_{index}"));
        let mut terlan_name = source_name_to_terlan_identifier(&name);
        while !names.insert(terlan_name.clone()) {
            terlan_name.push('_');
        }
        params.push(DomParamPlan {
            js_name: name,
            terlan_name,
            optional: false,
            rest: element.rest,
            terlan_type,
        });
    }
    let return_type = if alias.type_params.is_empty() {
        module_path.into()
    } else {
        format!("{module_path}[{}]", alias.type_params.join(", "))
    };
    Some(DomMethodPlan {
        partial: false,
        call_kind: TsCallKind::TuplePack,
        type_params: Vec::new(),
        type_constraints: Vec::new(),
        callable_bounds: alias
            .type_params
            .iter()
            .zip(&alias.type_constraints)
            .filter(|(_, constraint)| matches!(constraint, Some(TsTypeRef::CallableBound)))
            .map(|(name, _)| name.clone())
            .collect(),
        js_name: "pack".into(),
        terlan_name: "pack".into(),
        doc: None,
        optional: false,
        params,
        return_type,
        undefined_only: false,
    })
}

/// Returns a source label for a namespace-qualified alias.
fn qualified_alias_source(alias: &TsTypeAliasDeclaration) -> String {
    if alias.namespace.is_empty() {
        alias.name.clone()
    } else {
        format!("{}.{}", alias.namespace, alias.name)
    }
}

/// Returns why a TypeScript interface is reserved for a hand-authored bridge.
///
/// Inputs:
/// - `name`: TypeScript interface name.
///
/// Output:
/// - Skip reason detail when the interface must not be generated.
/// - `None` when normal generation may continue.
///
/// Transformation:
/// - Protects Terlan-owned bridge modules from full `lib.es5.d.ts` generation
///   while still recording a stable skip row for absent TypeScript interfaces.
fn reserved_bridge_interface_reason(name: &str) -> Option<&'static str> {
    match name {
        "Array" | "ArrayConstructor" | "String" | "StringConstructor" | "Number"
        | "NumberConstructor" | "Promise" | "PromiseConstructor" => {
            Some("hand-authored bridge wrapper")
        }
        _ => None,
    }
}

/// Returns the Terlan namespace used for one TypeScript binding.
///
/// Inputs:
/// - `namespace`: manifest-provided namespace, or an empty namespace for older
///   parser-only tests and fixtures.
///
/// Output:
/// - Namespace used to construct generated module paths and output paths.
///
/// Transformation:
/// - Preserves explicit manifest namespaces and falls back to the original DOM
///   namespace for direct parser fixtures that bypass the input manifest.
fn binding_namespace(namespace: &str) -> &str {
    if namespace.is_empty() {
        "std.js.Dom"
    } else {
        namespace
    }
}

/// Converts a Terlan namespace into a generated source directory.
///
/// Inputs:
/// - `namespace`: manifest-owned module namespace such as `std.js.Dom`.
///
/// Output:
/// - Repository-relative directory path.
///
/// Transformation:
/// - Maps dotted module namespace segments to the checked-in stdlib directory
///   layout used by generated TypeScript bindings.
fn namespace_to_source_dir(namespace: &str) -> String {
    namespace
        .split('.')
        .map(source_name_to_terlan_identifier)
        .collect::<Vec<_>>()
        .join("/")
}

/// Converts a parser-level unsupported member into a DOM skip row.
///
/// Inputs:
/// - `interface_name`: source TypeScript interface name.
/// - `unsupported`: parser adapter skip metadata.
///
/// Output:
/// - DOM-level skipped declaration row.
///
/// Transformation:
/// - Prefixes the member source with the interface name so generated skip
///   manifests explain every absent TypeScript member by source path.
fn map_unsupported_member(
    source: String,
    unsupported: &TsUnsupportedMember,
) -> DomSkippedDeclaration {
    DomSkippedDeclaration {
        source,
        reason: unsupported.reason,
        detail: unsupported.detail.clone(),
    }
}

/// Returns a unique skip-manifest source for an unsupported interface member.
fn unique_unsupported_member_source(
    interface_name: &str,
    unsupported: &TsUnsupportedMember,
    seen: &mut BTreeMap<String, usize>,
) -> String {
    let base = format!("{interface_name}.{}", unsupported.source);
    let count = seen.entry(base.clone()).or_insert(0);
    *count += 1;
    if *count == 1 {
        base
    } else {
        format!("{base}#{}", *count)
    }
}

/// Maps one TypeScript property into a DOM property plan.
///
/// Inputs:
/// - `interface_name`: source interface name for diagnostics.
/// - `property`: neutral property declaration.
/// - `skipped`: shared skipped-declaration accumulator.
///
/// Output:
/// - `Some(DomPropertyPlan)` when the property type maps successfully.
/// - `None` when mapping is skipped with diagnostics.
///
/// Transformation:
/// - Converts the TypeScript type and derives a Terlan `snake_case` member
///   name while preserving the original JavaScript name.
fn map_property(
    interface_name: &str,
    type_params: &[String],
    this_type: &TsTypeRef,
    property: &TsPropertyDeclaration,
    skipped: &mut Vec<DomSkippedDeclaration>,
) -> Option<DomPropertyPlan> {
    let ty = super::ts_parser_adapter::specialize_this_type(&property.ty, this_type);
    let mapping = map_ts_type_to_terlan(&ty);
    let mut terlan_type = mapped_type_or_skip(
        format!("{interface_name}.{}", property.name),
        mapping,
        skipped,
    )?;
    if property.optional && !terlan_type.starts_with("Option[") {
        terlan_type = format!("Option[{terlan_type}]");
    }

    Some(DomPropertyPlan {
        write_supported: writable_type(&ty, type_params),
        js_name: property.name.clone(),
        terlan_name: source_name_to_terlan_identifier(&property.name),
        doc: property.doc.clone(),
        readonly: property.readonly,
        optional: property.optional,
        undefined_only: undefined_only(&ty, property.optional),
        terlan_type,
    })
}

/// Maps one TypeScript method into a DOM method plan.
///
/// Inputs:
/// - `interface_name`: source interface name for diagnostics.
/// - `method`: neutral method declaration.
/// - `allow_partial`: allow omitting an optional suffix in declaration bindings.
/// - `skipped`: shared skipped-declaration accumulator.
///
/// Output:
/// - `Some(DomMethodPlan)` when the method or its callable prefix maps.
/// - `None` when a required type mapping is skipped with diagnostics.
///
/// Transformation:
/// - Converts method signature types and derives Terlan names without changing
///   the underlying JavaScript method name.
fn map_method(
    interface_name: &str,
    this_type: &TsTypeRef,
    method: &TsMethodDeclaration,
    allow_partial: bool,
    skipped: &mut Vec<DomSkippedDeclaration>,
) -> Option<DomMethodPlan> {
    let mut params = Vec::new();
    for (index, param) in method.params.iter().enumerate() {
        let mapped = map_param(interface_name, &method.name, this_type, param, skipped);
        match mapped {
            Some(mapped) => params.push(mapped),
            None if allow_partial && method.params[index..].iter().all(|param| param.optional) => {
                break;
            }
            None => return None,
        }
    }

    let return_type =
        super::ts_parser_adapter::specialize_this_type(&method.return_type, this_type);
    let return_type = mapped_type_or_skip(
        format!("{interface_name}.{} return", method.name),
        map_ts_type_to_terlan(&return_type),
        skipped,
    )?;

    let mut type_constraints = Vec::new();
    for (name, constraint) in method.type_params.iter().zip(&method.type_constraints) {
        let ty = if let Some(constraint) = constraint {
            let ty = super::ts_parser_adapter::specialize_this_type(constraint, this_type);
            Some(mapped_type_or_skip(
                format!("{interface_name}.{} constraint {name}", method.name),
                map_ts_type_to_terlan(&ty),
                skipped,
            )?)
        } else {
            None
        };
        type_constraints.push(ty);
    }

    Some(DomMethodPlan {
        partial: params.len() != method.params.len(),
        call_kind: method.call_kind,
        type_params: method.type_params.clone(),
        type_constraints,
        callable_bounds: Vec::new(),
        js_name: method.name.clone(),
        terlan_name: match method.call_kind {
            TsCallKind::Call => "call".into(),
            TsCallKind::Construct => "new".into(),
            TsCallKind::Method => source_name_to_terlan_identifier(&method.name),
            TsCallKind::TuplePack => unreachable!("tuple pack is mapped separately"),
        },
        doc: method.doc.clone(),
        optional: method.optional,
        params,
        return_type,
        undefined_only: undefined_only(&method.return_type, false),
    })
}

/// An optional generic value can itself be null; only its declared absence is None.
fn undefined_only(ty: &TsTypeRef, optional: bool) -> bool {
    fn contains(ty: &TsTypeRef, null: bool) -> bool {
        match ty {
            TsTypeRef::Null => null,
            TsTypeRef::Undefined => !null,
            TsTypeRef::Union(types) => types.iter().any(|ty| contains(ty, null)),
            _ => false,
        }
    }
    (optional || contains(ty, false)) && !contains(ty, true)
}

/// Maps one TypeScript method parameter into a DOM parameter plan.
///
/// Inputs:
/// - `interface_name`: source interface name for diagnostics.
/// - `method_name`: source method name for diagnostics.
/// - `param`: neutral parameter declaration.
/// - `skipped`: shared skipped-declaration accumulator.
///
/// Output:
/// - `Some(DomParamPlan)` when the parameter type maps successfully.
/// - `None` when mapping is skipped with diagnostics.
///
/// Transformation:
/// - Converts the parameter type and normalizes the generated Terlan parameter
///   name.
fn map_param(
    interface_name: &str,
    method_name: &str,
    this_type: &TsTypeRef,
    param: &TsParameterDeclaration,
    skipped: &mut Vec<DomSkippedDeclaration>,
) -> Option<DomParamPlan> {
    let ty = super::ts_parser_adapter::specialize_this_type(&param.ty, this_type);
    let terlan_type = mapped_type_or_skip(
        format!("{interface_name}.{method_name} parameter {}", param.name),
        map_ts_type_to_terlan(&ty),
        skipped,
    )?;

    Some(DomParamPlan {
        js_name: param.name.clone(),
        terlan_name: source_name_to_terlan_identifier(&param.name),
        optional: param.optional,
        rest: param.rest,
        terlan_type,
    })
}

/// Extracts a mapped Terlan type or records skip diagnostics.
///
/// Inputs:
/// - `source`: source declaration path for diagnostics.
/// - `mapping`: T0.3 type mapping result.
/// - `skipped`: shared skipped-declaration accumulator.
///
/// Output:
/// - `Some(String)` when a Terlan type was produced.
/// - `None` when one or more skip diagnostics were recorded.
///
/// Transformation:
/// - Converts type-level skip diagnostics into DOM declaration skip entries.
fn mapped_type_or_skip(
    source: String,
    mapping: TsTypeMapping,
    skipped: &mut Vec<DomSkippedDeclaration>,
) -> Option<String> {
    if let Some(terlan_type) = mapping.terlan_type {
        return Some(terlan_type);
    }

    for skip in mapping.skipped {
        skipped.push(skip_to_dom_skipped(source.clone(), skip));
    }
    None
}

/// Converts a type skip diagnostic into a DOM skip diagnostic.
///
/// Inputs:
/// - `source`: declaration path where the skip occurred.
/// - `skip`: type-level skip diagnostic.
///
/// Output:
/// - DOM-level skip diagnostic.
///
/// Transformation:
/// - Preserves the stable reason code and carries the source type label into
///   generated-manifest-ready detail text.
fn skip_to_dom_skipped(source: String, skip: TsTypeSkip) -> DomSkippedDeclaration {
    DomSkippedDeclaration {
        source,
        reason: skip.reason,
        detail: skip.source,
    }
}

// Read access may safely widen literal types; writes must preserve the full
// declaration contract. Opaque generic payloads retain their original type.
fn writable_type(ty: &TsTypeRef, type_params: &[String]) -> bool {
    use super::ts_type_mapping::TsPrimitiveType;
    match ty {
        TsTypeRef::Primitive(
            TsPrimitiveType::String | TsPrimitiveType::Number | TsPrimitiveType::Boolean,
        ) => true,
        TsTypeRef::Named(name) => type_params.contains(name),
        TsTypeRef::Array(inner) => writable_type(inner, type_params),
        TsTypeRef::Union(types) => types.iter().all(|ty| {
            matches!(ty, TsTypeRef::Null | TsTypeRef::Undefined) || writable_type(ty, type_params)
        }),
        _ => false,
    }
}
