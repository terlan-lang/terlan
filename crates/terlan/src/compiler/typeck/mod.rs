use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::terlan_hir::{
    syntax_module_output_to_interface, FunctionSymbol, ModuleInterface, ResolvedModule,
    TypeVisibility,
};
use crate::terlan_syntax::{
    extract_native_function_signatures, span::Span, SyntaxConstructorParamOutput,
    SyntaxDeclarationOutput, SyntaxDeclarationPayload, SyntaxExprKind, SyntaxExprOutput,
    SyntaxFunctionClauseOutput, SyntaxImplMethodOutput, SyntaxImportKind, SyntaxModuleOutput,
    SyntaxParamOutput, SyntaxPatternKind, SyntaxPatternOutput, SyntaxStructFieldOutput,
    SyntaxTypeOutput,
};

mod types;

pub(crate) use types::StructFieldVisibility;
pub use types::{pretty_type, DiagSeverity, Diagnostic, MapFieldType, Type, TypeVarId};

pub(crate) mod type_system;
use type_system::*;

mod raw_macros;
pub use raw_macros::{
    collect_syntax_raw_macro_diagnostics, collect_syntax_unsupported_raw_declaration_diagnostics,
    expand_syntax_raw_macros,
};

mod sql_forms;

mod import_maps;
use import_maps::*;

mod import_diagnostics;
use import_diagnostics::*;

mod field_visibility;
use field_visibility::*;

mod expression;
use expression::*;

mod named_args;
use named_args::*;

mod declarations;
use declarations::*;

mod signature_loading;
use signature_loading::*;

mod trait_conformance;
use trait_conformance::*;

mod html_typecheck;
use html_typecheck::*;

mod patterns;
use patterns::*;

mod trait_model;
use trait_model::*;

mod receiver_methods;
use receiver_methods::*;

mod type_instantiation;
use type_instantiation::*;

mod primitive_surface;
use primitive_surface::*;

mod constructors;
pub(crate) use constructors::alias_constructor_schemes;
use constructors::*;

mod core_ir;

pub use core_ir::*;
pub(crate) use core_ir::{core_type_from_body_variants, core_type_from_text};

mod core_sql_lowering;

/// Callable function type scheme used by expression inference.
///
/// Inputs:
/// - Local/imported function signature metadata after type parsing.
///
/// Output:
/// - Parameter types, return type, source generic parameter texts, and generic
///   trait bounds.
///
/// Transformation:
/// - Stores one overload candidate in a compact form that can be instantiated
///   during call resolution while retaining generic parameter spelling for
///   HKT variance checks at explicit call sites.
#[derive(Debug, Clone)]
struct FunctionScheme {
    params: Vec<Type>,
    ret: Type,
    generic_params: Vec<String>,
    bounds: Vec<FunctionBound>,
}

/// Generic trait bound attached to a function scheme.
///
/// Inputs:
/// - Parsed generic bound syntax from declarations or interfaces.
///
/// Output:
/// - Trait name plus type arguments required by generic dispatch.
///
/// Transformation:
/// - Converts source bound text into typechecker types for later satisfaction
///   checks.
#[derive(Debug, Clone)]
struct FunctionBound {
    trait_name: String,
    trait_args: Vec<Type>,
}

/// Constructor callable type scheme.
///
/// Inputs:
/// - Constructor declarations and eligible constructor-like alias shapes.
///
/// Output:
/// - Fixed/vararg parameter types, minimum arity, and return type.
///
/// Transformation:
/// - Represents constructor calls and patterns with the same overload matching
///   data needed by ordinary call checking.
#[derive(Debug, Clone)]
pub(crate) struct ConstructorScheme {
    param_names: Vec<String>,
    fixed_params: Vec<Type>,
    min_arity: usize,
    vararg: Option<Type>,
    ret: Type,
}

/// Template instantiation type scheme.
///
/// Inputs:
/// - Template declarations and imported template metadata.
///
/// Output:
/// - Property name to expected type map.
///
/// Transformation:
/// - Normalizes template props into a field lookup table for type checking
///   template instantiation expressions.
#[derive(Debug, Clone)]
struct TemplateScheme {
    prop_order: Vec<String>,
    props: HashMap<String, TemplatePropScheme>,
}

/// One template property typechecking scheme.
///
/// Inputs:
/// - Parsed template declaration property type and optional default expression.
///
/// Output:
/// - Expected property type plus structured default metadata.
///
/// Transformation:
/// - Keeps template declaration defaults beside their property types so
///   instantiation checking can accept omitted defaulted properties without
///   treating every template field as required.
#[derive(Debug, Clone)]
struct TemplatePropScheme {
    ty: Type,
    default: Option<SyntaxExprOutput>,
}

/// Local or imported type alias model.
///
/// Inputs:
/// - Type/opaque type declarations after parsing their body into `Type`.
///
/// Output:
/// - Type parameters, alias body, source constructor parameter labels, and
///   opacity flag.
///
/// Transformation:
/// - Keeps aliases available for expansion while preserving opacity decisions
///   for constructor and pattern validation. Constructor labels stay separate
///   from the runtime type model because tuple field labels are source-facing
///   call metadata, not structural runtime type identity.
#[derive(Debug, Clone)]
pub(crate) struct TypeAlias {
    params: Vec<TypeVarId>,
    param_variance: Vec<Variance>,
    body: Type,
    constructor_param_names: Vec<String>,
    is_opaque: bool,
}

/// Variance declared for one generic type parameter.
///
/// Inputs:
/// - Source type parameter spelling such as `T`, `+T`, or `-T`.
///
/// Output:
/// - Direction used by variance-aware subtype checks.
///
/// Transformation:
/// - Keeps the default invariant and makes covariance/contravariance explicit
///   so generic assignability never depends on inference magic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Variance {
    Invariant,
    Covariant,
    Contravariant,
}

/// Fully qualified imported type name.
///
/// Inputs:
/// - Resolver import metadata for visible types.
///
/// Output:
/// - Provider module and source type name.
///
/// Transformation:
/// - Gives diagnostics and constructor lookup a stable qualified identity for
///   imported type references.
#[derive(Debug, Clone)]
struct QualifiedTypeName {
    module: String,
    name: String,
}

/// Pre-collected inputs required by module type checking.
///
/// Inputs:
/// - Resolver output plus local/imported aliases, signatures, traits, macros,
///   templates, receiver methods, and collected diagnostics.
///
/// Output:
/// - Single bundle consumed by `type_check_syntax_module_with_inputs`.
///
/// Transformation:
/// - Separates collection from validation so each typechecking phase can be
///   tested and refactored independently.
#[derive(Debug, Clone)]
struct TypeCheckInputs<'a> {
    import_maps: TypeCheckImportMaps,
    local_aliases: HashMap<String, TypeAlias>,
    alias_extra_names: HashSet<String>,
    kind_diagnostics: Vec<Diagnostic>,
    macro_decl_diagnostics: Vec<Diagnostic>,
    trait_decl_diagnostics: Vec<Diagnostic>,
    trait_impl_coherence_diagnostics: Vec<Diagnostic>,
    trait_impl_signature_diagnostics: Vec<Diagnostic>,
    function_signatures: HashMap<(String, usize), Vec<FunctionScheme>>,
    constructor_signatures: HashMap<String, Vec<ConstructorScheme>>,
    struct_fields: HashMap<String, HashMap<String, Type>>,
    struct_field_visibility: HashMap<String, HashMap<String, StructFieldVisibility>>,
    receiver_methods: HashMap<(String, usize), Vec<ReceiverMethodDispatchSignature>>,
    template_schemes: HashMap<String, TemplateScheme>,
    syntax_function_module: &'a SyntaxModuleOutput,
    trait_signatures: HashMap<String, ParsedTraitSignature>,
    trait_method_calls: HashMap<(String, String), Vec<ResolvedTraitMethod>>,
}

/// Runs type checking using pre-collected module inputs.
///
/// Inputs:
/// - `resolved`: resolver output containing imported interfaces, local symbols,
///   and resolver diagnostics.
/// - `inputs`: collected type aliases, signatures, conformance diagnostics,
///   import maps, and syntax module reference.
///
/// Output:
/// - Diagnostics from resolver adaptation, declaration validation, expression
///   body checking, trait checks, and macro/kind checks.
///
/// Transformation:
/// - Builds the shared expression inference context from pre-collected inputs,
///   merges imported/local aliases, adapts resolver diagnostics to typechecker
///   diagnostics, and delegates callable body checking to the declaration
///   module.
fn type_check_syntax_module_with_inputs<'a>(
    resolved: &ResolvedModule,
    inputs: TypeCheckInputs<'a>,
) -> Vec<Diagnostic> {
    let mut diagnostics = resolved
        .diagnostics
        .iter()
        .map(|diag| Diagnostic {
            span: diag.span,
            message: diag.message.clone(),
            severity: DiagSeverity::Error,
        })
        .collect::<Vec<_>>();

    let local_aliases = inputs.local_aliases;
    diagnostics.extend(inputs.kind_diagnostics);
    diagnostics.extend(inputs.macro_decl_diagnostics);
    let trait_signatures = inputs.trait_signatures;
    diagnostics.extend(inputs.trait_impl_coherence_diagnostics);
    diagnostics.extend(inputs.trait_decl_diagnostics);
    diagnostics.extend(inputs.trait_impl_signature_diagnostics);
    if std::env::var("TYPL_TCDUMP").is_ok() {
        eprintln!("aliases: {:?}", local_aliases.keys().collect::<Vec<_>>());
    }
    let imported_type_aliases = imported_type_aliases(resolved);
    let mut aliases = imported_type_aliases.clone();
    aliases.extend(local_aliases.clone());

    let mut alias_names: HashSet<String> = aliases.keys().cloned().collect();
    alias_names.extend(resolved.imported_types.keys().cloned());
    let imported_type_names = imported_type_names(resolved);
    alias_names.extend(inputs.alias_extra_names);
    alias_names.extend(primitive_type_names());
    let function_signatures = inputs.function_signatures;
    let constructor_signatures = inputs.constructor_signatures;
    let struct_fields = inputs.struct_fields;
    let struct_field_visibility = inputs.struct_field_visibility;
    let receiver_methods = inputs.receiver_methods;
    let template_schemes = inputs.template_schemes;
    let import_maps = inputs.import_maps;
    let module_aliases = import_maps.module_aliases;
    let file_imports = import_maps.file_imports;
    let markdown_imports = import_maps.markdown_imports;
    let function_imports = import_maps.function_imports;
    let imported_type_names = imported_type_names;
    let constructor_aliases = imported_type_names.clone();
    let trait_method_calls = inputs.trait_method_calls;
    let trait_bound_impl_type_args = collect_trait_bound_impl_type_args(&trait_method_calls);
    let trait_lookup_cache = RefCell::new(TraitLookupCache::default());
    let expr_ctx = ExprInferContext {
        local_fns: &resolved.function_symbols,
        signatures: &function_signatures,
        interface_map: &resolved.interface_map,
        module_aliases: &module_aliases,
        file_imports: &file_imports,
        markdown_imports: &markdown_imports,
        function_imports: &function_imports,
        imported_type_names: &imported_type_names,
        constructor_aliases: &constructor_aliases,
        constructors: &constructor_signatures,
        templates: &template_schemes,
        aliases: &aliases,
        struct_fields: &struct_fields,
        struct_field_visibility: &struct_field_visibility,
        receiver_methods: &receiver_methods,
        trait_method_calls: &trait_method_calls,
        trait_bound_impl_type_args: &trait_bound_impl_type_args,
        trait_signatures: &trait_signatures,
        alias_names: &alias_names,
        current_bounds: &[],
        current_constructor_target: None,
        trait_lookup_cache: &trait_lookup_cache,
    };

    diagnostics.extend(check_syntax_constructor_param_defaults(
        inputs.syntax_function_module,
        &constructor_signatures,
        &aliases,
        &expr_ctx,
    ));
    diagnostics.extend(check_syntax_module_functions(
        inputs.syntax_function_module,
        &function_signatures,
        &constructor_signatures,
        &alias_names,
        &aliases,
        &imported_type_names,
        &imported_type_aliases,
        &local_aliases,
        &expr_ctx,
    ));

    diagnostics
}

/// Formal type checker entry point for compiler-facing syntax output.
///
/// This path must not adapt through the parser AST adapter.
pub fn type_check_syntax_module_output(
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
) -> Vec<Diagnostic> {
    let local_aliases = collect_syntax_type_aliases(module);
    let imported_aliases = imported_type_aliases(resolved);
    let imported_names = imported_type_names(resolved);
    let mut aliases = imported_aliases.clone();
    aliases.extend(local_aliases.clone());
    let local_type_names = collect_syntax_type_names(module);
    let mut alias_names = local_type_names.clone();
    alias_names.extend(imported_aliases.keys().cloned());
    alias_names.extend(resolved.imported_types.keys().cloned());
    alias_names.extend(collect_syntax_alias_extra_names(module));
    alias_names.extend(primitive_type_names());
    let trait_signatures = collect_syntax_trait_signatures(module, resolved);
    let trait_method_calls =
        collect_syntax_trait_method_calls(module, &alias_names, &trait_signatures, resolved);
    let mut trait_decl_diagnostics = check_syntax_trait_decls(module, &trait_signatures);
    trait_decl_diagnostics.extend(check_syntax_struct_includes(module, resolved));
    trait_decl_diagnostics.extend(check_syntax_declared_implements(module, &trait_signatures));
    let trait_impl_coherence_diagnostics = check_syntax_trait_impl_coherence(module);
    let trait_impl_signature_diagnostics =
        check_syntax_trait_impl_signatures(module, &trait_signatures);
    let receiver_method_diagnostics = check_syntax_receiver_methods(module, &local_type_names);

    let mut diagnostics = collect_syntax_unsupported_raw_declaration_diagnostics(module);
    diagnostics.extend(check_syntax_public_constructor_return_visibility(
        module,
        resolved,
        &alias_names,
    ));

    let mut struct_fields = collect_imported_struct_fields(resolved, &alias_names);
    struct_fields.extend(collect_syntax_struct_fields(module, &alias_names));
    let mut struct_field_visibility = collect_imported_struct_field_visibility(resolved);
    struct_field_visibility.extend(collect_syntax_struct_field_visibility(module));

    let inputs = TypeCheckInputs {
        import_maps: collect_syntax_import_maps(module, &resolved.interface_map),
        local_aliases: local_aliases.clone(),
        alias_extra_names: collect_syntax_alias_extra_names(module),
        kind_diagnostics: collect_syntax_kind_diagnostics(module, &trait_signatures, &aliases),
        macro_decl_diagnostics: check_syntax_macro_decl_signatures(module),
        trait_decl_diagnostics,
        trait_impl_coherence_diagnostics,
        trait_impl_signature_diagnostics,
        receiver_methods: collect_syntax_receiver_method_dispatch_signatures_with_imports(
            module,
            resolved,
            &alias_names,
            &imported_names,
            &imported_aliases,
            &local_aliases,
        ),
        function_signatures: collect_syntax_function_signatures(
            module,
            &alias_names,
            &imported_names,
            &imported_aliases,
            &local_aliases,
        ),
        constructor_signatures: collect_syntax_constructor_signatures(
            module,
            &alias_names,
            &imported_names,
            &imported_aliases,
            &aliases,
        ),
        struct_fields,
        struct_field_visibility,
        template_schemes: collect_syntax_template_schemes(module, &alias_names),
        syntax_function_module: module,
        trait_signatures,
        trait_method_calls,
    };
    diagnostics.extend(receiver_method_diagnostics);
    diagnostics.extend(type_check_syntax_module_with_inputs(resolved, inputs));

    diagnostics
}

mod core_expr_lowering;
mod core_expr_proof;
mod core_interface;
mod core_intrinsic_lowering;
mod core_lowering;
mod core_pattern_lowering;
mod core_proof;

pub use core_intrinsic_lowering::core_primitive_intrinsic_return_type;
pub use core_lowering::{lower_resolved_module_to_core, lower_syntax_module_output_to_core};
pub use core_sql_lowering::sql_query_core_expr_from_syntax;

#[cfg(test)]
pub(crate) use core_expr_lowering::core_expr_from_syntax;
#[cfg(test)]
pub(crate) use core_expr_proof::{
    constructor_chain_proof_coverage_policy, core_expr_is_lean_modeled, core_expr_proof_coverage,
    remote_call_is_promoted_to_lean_covered, remote_call_proof_coverage_policy,
};
#[cfg(test)]
pub(crate) use core_intrinsic_lowering::{
    core_io_effect_set, core_pure_effect_set, core_receiver_mutation_effect_set,
};
#[cfg(test)]
pub(crate) use core_pattern_lowering::{core_pattern_from_syntax, core_pattern_proof_coverage};
#[cfg(test)]
pub(crate) use core_proof::metadata::{
    core_module_proof_readiness, core_proof_readiness, CoreProofCoverageCounts,
    CoreTypePayloadCounts,
};

/// Expands struct inclusion into explicit child fields.
///
/// Inputs:
/// - `module`: mutable syntax-output module whose struct declarations may
///   contain `includes` parent names.
/// - `resolved`: resolved module context used to read imported parent struct
///   fields from module interfaces.
///
/// Output:
/// - Diagnostics for invalid parent references or inherited-field conflicts.
///
/// Transformation:
/// - Validates the `includes` clauses with the same rules as formal
///   typechecking, then prepends fields from local or imported parent structs
///   to each child struct.
fn expand_syntax_struct_includes(
    module: &mut SyntaxModuleOutput,
    resolved: &ResolvedModule,
) -> Vec<Diagnostic> {
    let mut diagnostics = check_syntax_struct_includes(module, resolved);
    if !diagnostics.is_empty() {
        return diagnostics;
    }

    let local_parent_fields = collect_local_syntax_struct_fields(module);
    let imported_parent_fields = collect_imported_syntax_struct_fields(resolved);
    let mut all_parent_fields = imported_parent_fields;
    all_parent_fields.extend(local_parent_fields);
    for declaration in &mut module.declarations {
        let SyntaxDeclarationPayload::Struct {
            name,
            includes,
            fields,
            ..
        } = &mut declaration.payload
        else {
            continue;
        };

        let mut inherited_fields = Vec::new();
        let mut field_names = fields
            .iter()
            .map(|field| field.name.clone())
            .collect::<HashSet<_>>();

        for parent_name in includes {
            let Some(parent_fields) = all_parent_fields.get(parent_name) else {
                continue;
            };

            for parent_field in parent_fields {
                if !field_names.insert(parent_field.name.clone()) {
                    diagnostics.push(Diagnostic {
                        span: declaration.span.into(),
                        message: format!(
                            "included struct `{}` field `{}` conflicts with declaration of struct `{}`",
                            parent_name, parent_field.name, name
                        ),
                        severity: DiagSeverity::Error,
                    });
                    continue;
                }
                inherited_fields.push(parent_field.clone());
            }
        }

        if diagnostics.is_empty() && !inherited_fields.is_empty() {
            inherited_fields.extend(fields.clone());
            *fields = inherited_fields;
        }
    }

    diagnostics
}

/// Runs the syntax-output include-expansion validation phase.
///
/// Inputs:
/// - `module`: compiler-facing syntax output to validate.
/// - `resolved`: resolved module context containing imported type names.
///
/// Output:
/// - A tuple containing the expanded syntax-output module and one diagnostic per
///   include validation or expansion failure.
///
/// Transformation:
/// - Validates struct includes clauses against visible struct names and copies
///   fields from local or imported parent structs into child structs.
pub fn expand_syntax_includes(
    mut module: SyntaxModuleOutput,
    resolved: &ResolvedModule,
) -> (SyntaxModuleOutput, Vec<Diagnostic>) {
    let diagnostics = expand_syntax_struct_includes(&mut module, resolved);
    (module, diagnostics)
}

/// Infers one expression type in the context of a syntax-output module.
///
/// Inputs:
/// - `expression`: expression node to infer.
/// - `module`: syntax-output module supplying local declarations and imports.
/// - `resolved`: resolver output supplying imported interfaces and symbols.
///
/// Output:
/// - Pair of inferred type and diagnostics collected while checking the module
///   and the expression itself.
///
/// Transformation:
/// - Reuses the same signature/import/conformance collection path as full
///   module typechecking, then builds an expression context and infers the
///   requested node with an empty local environment.
pub fn infer_syntax_expression_type(
    expression: &SyntaxExprOutput,
    module: &SyntaxModuleOutput,
    resolved: &ResolvedModule,
) -> (Type, Vec<Diagnostic>) {
    let mut diagnostics = type_check_syntax_module_output(module, resolved);

    let local_aliases = collect_syntax_type_aliases(module);
    let imported_aliases = imported_type_aliases(resolved);
    let imported_names = imported_type_names(resolved);
    let mut aliases = imported_aliases.clone();
    aliases.extend(local_aliases.clone());
    let mut alias_names = collect_syntax_type_names(module);
    alias_names.extend(imported_aliases.keys().cloned());
    alias_names.extend(resolved.imported_types.keys().cloned());
    alias_names.extend(collect_syntax_alias_extra_names(module));
    let trait_signatures = collect_syntax_trait_signatures(module, resolved);
    let trait_method_calls =
        collect_syntax_trait_method_calls(module, &alias_names, &trait_signatures, resolved);
    let function_signatures = collect_syntax_function_signatures(
        module,
        &alias_names,
        &imported_names,
        &imported_aliases,
        &local_aliases,
    );
    let constructor_signatures = collect_syntax_constructor_signatures(
        module,
        &alias_names,
        &imported_names,
        &imported_aliases,
        &aliases,
    );
    let mut struct_fields = collect_imported_struct_fields(resolved, &alias_names);
    struct_fields.extend(collect_syntax_struct_fields(module, &alias_names));
    let mut struct_field_visibility = collect_imported_struct_field_visibility(resolved);
    struct_field_visibility.extend(collect_syntax_struct_field_visibility(module));
    let receiver_methods = collect_syntax_receiver_method_dispatch_signatures_with_imports(
        module,
        resolved,
        &alias_names,
        &imported_names,
        &imported_aliases,
        &local_aliases,
    );
    let template_schemes = collect_syntax_template_schemes(module, &alias_names);
    let import_maps = collect_syntax_import_maps(module, &resolved.interface_map);
    let imported_type_names = imported_type_names(resolved);
    let constructor_aliases = imported_type_names.clone();
    let trait_bound_impl_type_args = collect_trait_bound_impl_type_args(&trait_method_calls);
    let trait_lookup_cache = RefCell::new(TraitLookupCache::default());
    let expr_ctx = ExprInferContext {
        local_fns: &resolved.function_symbols,
        signatures: &function_signatures,
        interface_map: &resolved.interface_map,
        module_aliases: &import_maps.module_aliases,
        file_imports: &import_maps.file_imports,
        markdown_imports: &import_maps.markdown_imports,
        function_imports: &import_maps.function_imports,
        imported_type_names: &imported_type_names,
        constructor_aliases: &constructor_aliases,
        constructors: &constructor_signatures,
        templates: &template_schemes,
        aliases: &aliases,
        struct_fields: &struct_fields,
        struct_field_visibility: &struct_field_visibility,
        receiver_methods: &receiver_methods,
        trait_method_calls: &trait_method_calls,
        trait_bound_impl_type_args: &trait_bound_impl_type_args,
        trait_signatures: &trait_signatures,
        alias_names: &alias_names,
        current_bounds: &[],
        current_constructor_target: None,
        trait_lookup_cache: &trait_lookup_cache,
    };

    let locals = HashMap::new();
    let mut subst = HashMap::new();
    let mut expression_errors = Vec::new();
    let ty = infer_syntax_expr(
        expression,
        &locals,
        &expr_ctx,
        &mut subst,
        &mut expression_errors,
    );

    diagnostics.extend(
        expression_errors
            .into_iter()
            .map(|message| expression_error_to_diagnostic(message, Span::new(0, 0))),
    );

    (
        apply_subst(&expand_type_aliases(&ty, &aliases), &subst),
        diagnostics,
    )
}

/// Normalizes generic type parameter text.
///
/// Inputs:
/// - `param`: source type parameter text, possibly with variance or bounds.
///
/// Output:
/// - Bare type parameter name.
///
/// Transformation:
/// - Removes leading variance markers and discards any inline bracketed suffix
///   so signature and interface loaders share one stable type-variable key.
fn normalize_type_param_name(param: &str) -> String {
    let trimmed = param.trim().trim_start_matches('-').trim_start_matches('+');
    if let Some(open) = trimmed.find('[') {
        trimmed[..open].trim().to_string()
    } else {
        trimmed.to_string()
    }
}

/// Extracts variance from one generic type parameter declaration.
///
/// Inputs:
/// - `param`: source type parameter text, possibly variance-prefixed.
///
/// Output:
/// - `Covariant` for `+T`, `Contravariant` for `-T`, and `Invariant`
///   otherwise.
///
/// Transformation:
/// - Reads only the leading marker and deliberately ignores higher-kind slot
///   text so alias-level parameter variance remains independent of HKT slot
///   variance.
fn type_param_variance(param: &str) -> Variance {
    match param.trim().chars().next() {
        Some('+') => Variance::Covariant,
        Some('-') => Variance::Contravariant,
        _ => Variance::Invariant,
    }
}

/// Extracts variance metadata for generic type parameters.
///
/// Inputs:
/// - `params`: source type parameter texts from a type declaration or
///   interface summary.
///
/// Output:
/// - One variance entry per parameter in declaration order.
///
/// Transformation:
/// - Maps each parameter through `type_param_variance`, preserving arity and
///   keeping invariant as the default.
fn type_param_variances(params: &[String]) -> Vec<Variance> {
    params
        .iter()
        .map(|param| type_param_variance(param))
        .collect()
}

#[cfg(test)]
mod expression_test;

#[cfg(test)]
mod import_test;

#[cfg(test)]
mod core_lowering_test;

#[cfg(test)]
mod core_expr_test;

#[cfg(test)]
mod core_control_flow_test;

#[cfg(test)]
mod core_intrinsic_test;

#[cfg(test)]
mod constructor_test;

#[cfg(test)]
mod adversarial_test;
#[cfg(test)]
mod diagnostic_test;
#[cfg(test)]
mod macro_test;

#[cfg(test)]
mod pattern_test;

#[cfg(test)]
mod primitive_test;

#[cfg(test)]
mod receiver_method_test;
#[cfg(test)]
mod sql_forms_test;
#[cfg(test)]
mod std_contract_test;
#[cfg(test)]
mod struct_test;
#[cfg(test)]
mod test_support;

#[cfg(test)]
mod trait_test;

#[cfg(test)]
mod type_model_test;
