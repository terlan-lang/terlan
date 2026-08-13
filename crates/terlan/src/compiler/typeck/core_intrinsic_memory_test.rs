use super::*;
use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;

/// Returns one named function's typed Core body.
fn body<'a>(core: &'a CoreModule, name: &str) -> &'a CoreExpr {
    core.functions
        .iter()
        .find(|function| function.name == name)
        .and_then(|function| function.clauses.first())
        .and_then(|clause| clause.body.core_expr.as_ref())
        .expect("typed Core body")
}

#[test]
fn explicit_memory_type_arguments_survive_core_lowering() {
    let module = parse_module_as_syntax_output(
        "module core_memory_intrinsics.\n\n\
         import std.core.Memory.\n\
         import type std.core.Memory.{Layout}.\n\n\
         pub int_layout(): Layout -> Memory.layout_of[Int]().\n\
         pub string_shallow(value: String): Int -> Memory.shallow_size[String](value).\n\
         pub string_retained(value: String): Int -> Memory.retained_size[String](value).\n",
    )
    .expect("parse Memory intrinsic fixture");
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    assert!(matches!(
        body(&core, "int_layout"),
        CoreExpr::Intrinsic(CoreIntrinsicCall {
            id: CoreIntrinsicId::MemoryLayoutOf(CoreType::Int),
            args,
            return_type: CoreType::Named(layout),
            ..
        }) if args.is_empty() && layout == "std.core.Memory.Layout"
    ));
    assert!(matches!(
        body(&core, "string_shallow"),
        CoreExpr::Intrinsic(CoreIntrinsicCall {
            id: CoreIntrinsicId::MemoryShallowSize(CoreType::String),
            args,
            return_type: CoreType::Int,
            ..
        }) if args == &vec![CoreExpr::Var("value".to_string())]
    ));
    assert!(matches!(
        body(&core, "string_retained"),
        CoreExpr::Intrinsic(CoreIntrinsicCall {
            id: CoreIntrinsicId::MemoryRetainedSize(CoreType::String),
            args,
            return_type: CoreType::Int,
            ..
        }) if args == &vec![CoreExpr::Var("value".to_string())]
    ));
    assert!(core.contract_text().contains("core.memory.layout_of[Int]"));
}

#[test]
fn explicit_list_element_type_survives_core_lowering() {
    let module = parse_module_as_syntax_output(
        "module core_list_intrinsics.\n\n\
         import std.collections.List.\n\n\
         pub strings(): List[String] -> List.new[String]().\n",
    )
    .expect("parse List intrinsic fixture");
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);

    assert!(matches!(
        body(&core, "strings"),
        CoreExpr::Intrinsic(CoreIntrinsicCall {
            id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListNew),
            args,
            return_type: CoreType::List(element),
            ..
        }) if args.is_empty() && element.as_ref() == &CoreType::String
    ));
}
