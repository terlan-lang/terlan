pub(super) use std::collections::{HashMap, HashSet};

pub(super) use crate::terlan_purity::infer_body_available_pure_callables;
pub(super) use crate::terlan_syntax::{
    extract_native_function_signatures, span::Span, SyntaxDeclarationOutput,
    SyntaxDeclarationPayload, SyntaxExprKind, SyntaxModuleOutput, SyntaxParamOutput,
    SyntaxSourceKind,
};

mod imported_type_refs;
mod imports;
mod interface_conversion;
mod interface_loading;
mod interface_render;
mod model;
mod naming;
mod resolution;
mod shapes;
#[cfg(test)]
#[path = "shapes_test.rs"]
#[cfg(test)]
mod shapes_test;
#[cfg(test)]
pub(crate) mod test_support;

pub(crate) use imported_type_refs::qualify_syntax_type_text;
pub use interface_loading::{
    load_discovery_interfaces_for_symbol_from_file_set, load_imported_interfaces_from_file_set,
    load_interfaces_from_dir, load_interfaces_from_file_set, parse_interface_dependency_entries,
    parse_interface_file,
};
pub use model::{
    ConstFunctionSignature, ConstantSignature, ConstructorSignature, Diagnostic,
    ExpressionMacroSignature, FunctionSignature, FunctionSymbol, ImportedItem, ModuleInterface,
    ParamSignature, ResolveResult, ResolvedModule, ShapeSignature, StructFieldSignature,
    TraitConformanceSignature, TraitConformanceSource, TraitConstantSignature,
    TraitMethodSignature, TraitSignature, TypeVisibility, ValuedUnionArmSignature,
    ValuedUnionSignature,
};
pub use naming::{
    identifier_to_snake, module_path_to_native_boundary_module, source_name_to_terlan_identifier,
};
pub use shapes::expand_syntax_shape_imports;
#[cfg(test)]
pub(crate) use test_support::checked_in_std_interfaces_for_module;

pub use resolution::{
    resolve_syntax_module_output, resolve_syntax_module_output_with_interfaces,
    syntax_module_output_to_interface,
};
