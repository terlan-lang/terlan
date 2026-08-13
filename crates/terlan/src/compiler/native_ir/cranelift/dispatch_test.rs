//! Executable regression coverage for sparse exported-image dispatch.

use std::collections::BTreeSet;

use object::{Object, ObjectSymbol};

use crate::{
    terlan_hir::resolve_syntax_module_output, terlan_syntax::parse_module_as_syntax_output,
    terlan_typeck::lower_syntax_module_output_to_core,
};

use super::super::{
    emit_native_application_object,
    native_object_test_support::{
        assert_managed_native_object_invocations, NativeObjectInvocation,
    },
    status, NativeModule,
};

#[test]
fn table_dispatch_routes_large_inventories_without_per_export_code_growth() {
    let mut source = String::from("module native_sparse_export_dispatch.\n\n");
    for index in 0..257 {
        source.push_str(&format!("pub value_{index}(): Int -> {index}.\n"));
    }
    let syntax = parse_module_as_syntax_output(&source).expect("parse sparse dispatch fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("lower sparse dispatch fixture");
    let exports = modules
        .iter()
        .flat_map(|module| &module.functions)
        .map(|function| (function.name.as_str(), function.export_id))
        .collect::<Vec<_>>();
    let known_ids = exports
        .iter()
        .map(|(_, export_id)| *export_id)
        .collect::<BTreeSet<_>>();
    let unknown_id = (0_u64..)
        .find(|candidate| !known_ids.contains(candidate))
        .expect("unknown export id");
    let invocation_for = |index: usize| NativeObjectInvocation {
        export_id: exports
            .iter()
            .find(|(name, _)| *name == format!("value_{index}"))
            .expect("fixture export")
            .1,
        arguments: Vec::new(),
        expected_status: status::OK,
        expected_result: Some(index as i64),
    };
    let invocations = [
        invocation_for(0),
        invocation_for(128),
        invocation_for(256),
        NativeObjectInvocation {
            export_id: unknown_id,
            arguments: Vec::new(),
            expected_status: status::UNKNOWN_EXPORT,
            expected_result: None,
        },
        NativeObjectInvocation {
            export_id: invocation_for(128).export_id,
            arguments: vec![1],
            expected_status: status::ARITY,
            expected_result: None,
        },
    ];
    let object = emit_native_application_object("native_sparse_export_dispatch", &modules)
        .expect("emit sparse dispatch fixture");

    let parsed = object::File::parse(object.as_slice()).expect("parse native dispatch object");
    let dispatch = parsed
        .symbols()
        .find(|symbol| symbol.name().ok() == Some("terlan_native_dispatch_v3"))
        .expect("exported dispatch symbol");
    assert!(
        dispatch.size() < 65_536,
        "dispatch code must be shared by ABI shape, not repeated for every export: {} bytes",
        dispatch.size()
    );
    let index = parsed
        .symbols()
        .find(|symbol| symbol.name().ok() == Some("terlan_native_dispatch_index_v2"))
        .expect("immutable dispatch index symbol");
    let records = parsed
        .symbols()
        .find(|symbol| symbol.name().ok() == Some("terlan_native_dispatch_records_v2"))
        .expect("dense dispatch records symbol");
    assert!(
        parsed
            .symbols()
            .all(|symbol| symbol.name().ok() != Some("terlan_native_dispatch_rare_v1")),
        "common call shapes must not pay for the cold dispatcher"
    );
    let expected_index_slots = exports.len().saturating_mul(2).max(2).next_power_of_two();
    assert_eq!(
        index.size(),
        expected_index_slots as u64 * 4,
        "the bounded-load-factor index must use one compact u32 per sparse slot"
    );
    assert_eq!(
        records.size(),
        exports.len() as u64 * 24,
        "dense records must store only occupied export metadata"
    );
    assert!(
        index.size() + records.size() < expected_index_slots as u64 * 32,
        "the two-level layout must remain smaller than full sparse records"
    );

    assert_managed_native_object_invocations(
        "native-sparse-export-dispatch",
        &modules,
        &object,
        &invocations,
    );
}

#[test]
fn rare_large_arity_shapes_use_one_out_of_line_packed_dispatcher() {
    let source = r#"
module native_rare_shape_dispatch.

pub sum9(
    a: Int,
    b: Int,
    c: Int,
    d: Int,
    e: Int,
    f: Int,
    g: Int,
    h: Int,
    i: Int
): Int -> a + b + c + d + e + f + g + h + i.
"#;
    let syntax = parse_module_as_syntax_output(source).expect("parse rare dispatch fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("lower rare dispatch fixture");
    let sum9 = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "sum9")
        .expect("sum9 export");
    let object = emit_native_application_object("native_rare_shape_dispatch", &modules)
        .expect("emit rare dispatch fixture");

    let parsed = object::File::parse(object.as_slice()).expect("parse rare dispatch object");
    let rare = parsed
        .symbols()
        .find(|symbol| symbol.name().ok() == Some("terlan_native_dispatch_rare_v1"))
        .expect("one image-local cold dispatcher");
    assert!(
        rare.size() > 0,
        "cold dispatcher must contain executable code"
    );
    assert_eq!(
        parsed
            .symbols()
            .filter(|symbol| symbol.name().ok() == Some("terlan_native_dispatch_rare_v1"))
            .count(),
        1,
        "all uncommon ABI shapes must share one cold dispatcher"
    );

    assert_managed_native_object_invocations(
        "native-rare-shape-dispatch",
        &modules,
        &object,
        &[NativeObjectInvocation {
            export_id: sum9.export_id,
            arguments: vec![1, 2, 3, 4, 5, 6, 7, 8, 9],
            expected_status: status::OK,
            expected_result: Some(45),
        }],
    );
}
