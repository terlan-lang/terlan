//! Explicit collection type arguments must survive intrinsic lowering.

use super::*;
use crate::terlan_hir::{
    checked_in_std_interfaces_for_module, resolve_syntax_module_output_with_interfaces,
};
use crate::terlan_syntax::parse_module_as_syntax_output;

#[test]
fn typed_collection_constructors_preserve_all_type_arguments() {
    let syntax = parse_module_as_syntax_output(
        "module typed_collection_constructors.\n\
         import std.collections.{List, Map, Set}.\n\
         pub list(): List[Int] -> List.new[Int]().\n\
         pub map(): Map[String, Int] -> Map.new[String, Int]().\n\
         pub set(): Set[String] -> Set.new[String]().\n",
    )
    .expect("typed constructor source");
    let interfaces = checked_in_std_interfaces_for_module(&syntax);
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    for (name, intrinsic, expected) in [
        (
            "list",
            CorePrimitiveIntrinsic::ListNew,
            CoreType::List(Box::new(CoreType::Int)),
        ),
        (
            "map",
            CorePrimitiveIntrinsic::MapNew,
            CoreType::Apply {
                constructor: "Map".into(),
                args: vec![CoreType::String, CoreType::Int],
            },
        ),
        (
            "set",
            CorePrimitiveIntrinsic::SetNew,
            CoreType::Apply {
                constructor: "Set".into(),
                args: vec![CoreType::String],
            },
        ),
    ] {
        let function = core
            .functions
            .iter()
            .find(|function| function.name == name)
            .expect(name);
        let Some(CoreExpr::Intrinsic(call)) = &function.clauses[0].body.core_expr else {
            panic!("{name}: expected typed intrinsic");
        };
        assert_eq!(call.id, CoreIntrinsicId::Primitive(intrinsic));
        assert_eq!(call.return_type, expected);
        assert!(call.args.is_empty());
        assert_eq!(call.effects, core_pure_effect_set());
    }
}
