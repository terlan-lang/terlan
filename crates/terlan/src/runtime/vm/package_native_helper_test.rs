use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use super::{
    accelerator_resource_handles, decode_payload, dispatch_vm_capability,
    dispatch_vm_capability_with_program_arguments, encode_argument, helper_environment_namespace,
    package_helper_environment, package_operation_namespace, ReplValue,
};
use crate::compiler::accelerator::{AcceleratorResourceClass, AcceleratorResourceRole};
use crate::runtime::native_image::TvmBoundaryType;
use crate::runtime::vm::pure_native::PureNativeCapabilityRequest;
use crate::terlan_native_boundary::term::{NativeBoundaryReplyTerm, NativeBoundaryTerm};

#[test]
fn package_helper_decodes_every_generated_primitive_reply() {
    assert_eq!(
        decode_payload(&["ok_float", "-2.5"]),
        Ok(ReplValue::Float("-2.5".to_string()))
    );
    assert_eq!(
        decode_payload(&["ok_string", &STANDARD.encode("ready")]),
        Ok(ReplValue::String("ready".to_string()))
    );
    assert_eq!(
        decode_payload(&["ok_atom", &STANDARD.encode("ready")]),
        Ok(ReplValue::Atom("ready".to_string()))
    );
    assert_eq!(
        decode_payload(&["ok_bools", "true,false"]),
        Ok(ReplValue::List(vec![
            ReplValue::Bool(true),
            ReplValue::Bool(false),
        ]))
    );
    let strings = format!(
        "{},{},{}",
        STANDARD.encode("positive"),
        STANDARD.encode(""),
        STANDARD.encode("ready")
    );
    assert_eq!(
        decode_payload(&["ok_strings", &strings]),
        Ok(ReplValue::List(vec![
            ReplValue::String("positive".to_string()),
            ReplValue::String(String::new()),
            ReplValue::String("ready".to_string()),
        ]))
    );
}

#[test]
fn package_helper_decodes_optional_owned_resource_replies() {
    assert_eq!(
        decode_payload(&["ok_none"]),
        Ok(ReplValue::Record {
            name: "None".to_string(),
            fields: Vec::new(),
        })
    );
    let owner = STANDARD.encode("actor.owner");
    let type_name = STANDARD.encode("pytorch.Tensor.Tensor");
    let value =
        decode_payload(&["ok_some_handle", &owner, "7", "3", &type_name]).expect("optional handle");
    assert!(matches!(
        value,
        ReplValue::Record { name, fields }
            if name == "Some"
                && matches!(
                    fields.as_slice(),
                    [(field, ReplValue::Record { name, .. })]
                        if field == "value" && name == "Tensor"
                )
    ));
}

#[test]
fn package_helper_decodes_polars_schema_records() {
    let payload = format!(
        "{}:{},{}:{}",
        STANDARD.encode("feature_a"),
        STANDARD.encode("Float64"),
        STANDARD.encode("label"),
        STANDARD.encode("Int64")
    );
    let schema = decode_payload(&["ok_schema", &payload]).expect("schema");
    assert!(matches!(
        schema,
        ReplValue::List(ref entries)
            if entries.len() == 2
                && matches!(
                    &entries[0],
                    ReplValue::Record { name, fields }
                        if name == "ColumnSchema"
                            && fields
                                == &vec![
                                    (
                                        "name".to_string(),
                                        ReplValue::String("feature_a".to_string())
                                    ),
                                    (
                                        "data_type".to_string(),
                                        ReplValue::String("Float64".to_string())
                                    ),
                                ]
                )
    ));
    assert_eq!(
        decode_payload(&["ok_schema"]),
        Ok(ReplValue::List(Vec::new()))
    );
    assert!(decode_payload(&["ok_schema", "missing-separator"]).is_err());
}

#[test]
fn package_helper_decodes_typed_result_envelopes() {
    let owner = STANDARD.encode("worker-1");
    let type_name = STANDARD.encode("polars.DataFrame.DataFrame");
    let handle =
        decode_payload(&["result_ok_handle", &owner, "7", "3", &type_name]).expect("result handle");
    assert!(matches!(
        handle,
        ReplValue::Record { ref name, ref fields }
            if name == "Ok"
                && matches!(
                    fields.as_slice(),
                    [(field, ReplValue::Record { name: handle_name, .. })]
                        if field == "value" && handle_name == "DataFrame"
                )
    ));

    let bytes = decode_payload(&["result_ok_bytes", &STANDARD.encode([0_u8, 127, 255])])
        .expect("result Bytes");
    assert!(matches!(
        bytes,
        ReplValue::Record { ref name, ref fields }
            if name == "Ok"
                && matches!(
                    fields.as_slice(),
                    [(field, ReplValue::Bytes(value))]
                        if field == "value" && value.as_ref() == [0, 127, 255]
                )
    ));
    assert!(matches!(
        decode_payload(&["result_ok_bytes"]).expect("empty result Bytes"),
        ReplValue::Record { ref name, ref fields }
            if name == "Ok"
                && matches!(
                    fields.as_slice(),
                    [(field, ReplValue::Bytes(value))]
                        if field == "value" && value.is_empty()
                )
    ));

    let error = decode_payload(&[
        "result_err",
        &STANDARD.encode("missing_column"),
        &STANDARD.encode("column `label` is missing"),
    ])
    .expect("typed result error");
    assert!(matches!(
        error,
        ReplValue::Record { ref name, ref fields }
            if name == "Err"
                && matches!(
                    fields.as_slice(),
                    [(field, ReplValue::Record { name: error_name, .. })]
                        if field == "reason" && error_name == "Error"
                )
    ));

    for (kind, values, expected_len) in [
        ("result_ok_ints", "1,-2", 2),
        ("result_ok_floats", "1.5,-2.25", 2),
        ("result_ok_bools", "true,false", 2),
    ] {
        assert!(matches!(
            decode_payload(&[kind, values]).expect("typed result list"),
            ReplValue::Record { ref name, ref fields }
                if name == "Ok"
                    && matches!(
                        fields.as_slice(),
                        [(field, ReplValue::List(values))]
                            if field == "value" && values.len() == expected_len
                    )
        ));
    }

    let encoded_strings = format!("{},{}", STANDARD.encode("one"), STANDARD.encode("two"));
    assert!(matches!(
        decode_payload(&["result_ok_strings", &encoded_strings]).expect("typed String list"),
        ReplValue::Record { ref name, ref fields }
            if name == "Ok"
                && matches!(
                    fields.as_slice(),
                    [(field, ReplValue::List(values))]
                        if field == "value" && values.len() == 2
                )
    ));
}

#[test]
fn package_helper_projects_cuda_handles_into_canonical_vm_resources() {
    let owner = STANDARD.encode("actor.owner");
    let type_name = STANDARD.encode("cuda.Buffer.Buffer");
    let value = decode_payload(&["ok_handle", &owner, "7", "3", &type_name]).expect("CUDA handle");
    let handles = accelerator_resource_handles(&value).expect("canonical handles");
    assert_eq!(handles.len(), 1);
    assert_eq!(handles[0].id.slot, 7);
    assert_eq!(handles[0].id.generation, 3);
    assert_eq!(handles[0].class, AcceleratorResourceClass::Allocation);
    assert!(matches!(
        handles[0].role,
        AcceleratorResourceRole::Owned { .. }
    ));

    let non_accelerator = decode_payload(&[
        "ok_handle",
        &owner,
        "8",
        "1",
        &STANDARD.encode("polars.DataFrame.DataFrame"),
    ])
    .expect("ordinary native handle");
    assert!(accelerator_resource_handles(&non_accelerator)
        .expect("non-accelerator projection")
        .is_empty());
}

#[test]
fn package_helper_canonicalizes_opaque_accelerator_resource_principals() {
    let value = decode_payload(&[
        "ok_handle",
        &STANDARD.encode("INVALID OWNER"),
        "7",
        "3",
        &STANDARD.encode("cuda.Stream.Stream"),
    ])
    .expect("wire handle");
    let handles = accelerator_resource_handles(&value).expect("canonical principal");
    assert!(matches!(
        &handles[0].role,
        AcceleratorResourceRole::Owned { principal }
            if principal.as_str().starts_with("cuda.")
                && !principal.as_str().contains(' ')
    ));
}

#[test]
fn package_helper_rejects_malformed_generated_primitive_replies() {
    for payload in [
        ["ok_float", "not-a-float"],
        ["ok_bool", "not-a-bool"],
        ["ok_bools", "true,not-a-bool"],
        ["ok_atom", "%%%"],
        ["ok_string", "%%%"],
        ["result_ok_bytes", "%%%"],
        ["result_ok_ints", "not-an-int"],
        ["result_ok_floats", "not-a-float"],
        ["result_ok_bools", "not-a-bool"],
    ] {
        let error = decode_payload(&payload).expect_err("malformed helper payload must fail");
        assert!(error.contains("error[native_helper_protocol]"), "{error}");
    }
}

#[test]
fn package_helper_preserves_empty_generated_lists() {
    for kind in ["ok_ints", "ok_floats", "ok_bools", "ok_strings"] {
        assert_eq!(
            decode_payload(&[kind]),
            Ok(ReplValue::List(Vec::new())),
            "{kind}"
        );
    }
    for kind in [
        "result_ok_ints",
        "result_ok_floats",
        "result_ok_bools",
        "result_ok_strings",
    ] {
        assert!(matches!(
            decode_payload(&[kind]).expect("empty typed result list"),
            ReplValue::Record { ref name, ref fields }
                if name == "Ok"
                    && matches!(
                        fields.as_slice(),
                        [(field, ReplValue::List(values))]
                            if field == "value" && values.is_empty()
                    )
        ));
    }
}

#[test]
fn package_helper_encodes_generated_bool_lists() {
    assert_eq!(
        encode_argument(&ReplValue::List(vec![
            ReplValue::Bool(true),
            ReplValue::Bool(false),
        ])),
        Ok("lb:true,false".to_string())
    );
}

#[test]
fn package_helper_round_trips_lists_of_opaque_resources() {
    let owner = STANDARD.encode("worker");
    let device_type = STANDARD.encode("cuda.Device.Device");
    let buffer_type = STANDARD.encode("cuda.Buffer.Buffer");
    let devices = decode_payload(&[
        "ok_handles",
        &format!("{owner}:7:3:{device_type},{owner}:8:4:{buffer_type}"),
    ])
    .expect("resource list");
    assert_eq!(
        encode_argument(&devices),
        Ok(format!(
            "lh:{owner}:7:3:{device_type},{owner}:8:4:{buffer_type}"
        ))
    );
}

#[test]
fn package_helper_rejects_malformed_resource_lists() {
    assert!(decode_payload(&["ok_handles", "missing:fields"]).is_err());
}

#[test]
fn package_helper_encodes_generated_string_lists() {
    assert_eq!(
        encode_argument(&ReplValue::List(vec![
            ReplValue::String("feature_a".to_string()),
            ReplValue::String("feature b".to_string()),
        ])),
        Ok(format!(
            "ls:{},{}",
            STANDARD.encode("feature_a"),
            STANDARD.encode("feature b"),
        ))
    );
}

#[test]
fn package_helper_encodes_nested_string_rows() {
    assert_eq!(
        encode_argument(&ReplValue::List(vec![
            ReplValue::List(vec![
                ReplValue::String("Ada".to_string()),
                ReplValue::StringBytes(b"London".to_vec().into()),
            ]),
            ReplValue::List(Vec::new()),
        ])),
        Ok(format!(
            "lss:{},{}",
            STANDARD.encode(format!(
                "2|{},{}",
                STANDARD.encode("Ada"),
                STANDARD.encode("London")
            )),
            STANDARD.encode("0|"),
        ))
    );
}

#[test]
fn package_helper_encodes_managed_nullable_primitive_lists() {
    let some = |value| ReplValue::Record {
        name: "Some".to_string(),
        fields: vec![("value".to_string(), value)],
    };
    let none = || ReplValue::Record {
        name: "None".to_string(),
        fields: Vec::new(),
    };

    assert_eq!(
        encode_argument(&ReplValue::List(vec![
            some(ReplValue::String("Ada".to_string())),
            none(),
        ])),
        Ok(format!("los:s{},n", STANDARD.encode("Ada")))
    );
    assert_eq!(
        encode_argument(&ReplValue::List(vec![some(ReplValue::Int(-7)), none(),])),
        Ok("loi:v-7,n".to_string())
    );
    assert_eq!(
        encode_argument(&ReplValue::List(vec![
            none(),
            some(ReplValue::Float("1.5".to_string())),
        ])),
        Ok("lof:n,v1.5".to_string())
    );
    assert_eq!(
        encode_argument(&ReplValue::List(vec![some(ReplValue::Bool(true)), none(),])),
        Ok("lob:vtrue,n".to_string())
    );
    assert_eq!(
        encode_argument(&ReplValue::List(vec![none(), none()])),
        Ok("lon:2".to_string())
    );
}

#[test]
fn package_helper_rejects_mixed_nullable_payload_types() {
    let values = ReplValue::List(vec![
        ReplValue::Record {
            name: "Some".to_string(),
            fields: vec![("value".to_string(), ReplValue::Int(1))],
        },
        ReplValue::Record {
            name: "Some".to_string(),
            fields: vec![("value".to_string(), ReplValue::Bool(true))],
        },
    ]);

    assert_eq!(
        encode_argument(&values).unwrap_err().to_string(),
        "error[native_helper_argument]: nullable list mixes payload types"
    );
}

#[test]
fn vm_owned_capability_dispatch_completes_without_replaying_the_suspension() {
    let request = PureNativeCapabilityRequest {
        capability: "filesystem".to_string(),
        operation: "std.io.file.exists".to_string(),
        arguments: vec![NativeBoundaryTerm::Text(".".to_string())],
        package_arguments: None,
        result_type: TvmBoundaryType::Bool,
    };

    assert_eq!(
        dispatch_vm_capability(&request),
        Ok(NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Bool(true)))
    );
}

#[test]
fn vm_owned_batch_text_read_returns_typed_records_and_exact_error_path() {
    let root = crate::support::test_fs::temp_dir("package_native_helper", "batch_text_read");
    let present = root.join("present.txt");
    let missing = root.join("missing.txt");
    std::fs::write(&present, "present").expect("write batch fixture");
    let request = |paths: Vec<NativeBoundaryTerm>| PureNativeCapabilityRequest {
        capability: "filesystem".to_string(),
        operation: "std.io.file.read_text_many".to_string(),
        arguments: vec![NativeBoundaryTerm::List(paths)],
        package_arguments: None,
        result_type: TvmBoundaryType::Managed([0; 16]),
    };

    assert!(matches!(
        dispatch_vm_capability(&request(vec![NativeBoundaryTerm::Text(
            present.to_string_lossy().into_owned()
        )])),
        Ok(NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Record {
            name,
            fields
        })) if name == "Ok" && matches!(
            &fields[0].1,
            NativeBoundaryTerm::List(files) if matches!(
                &files[0],
                NativeBoundaryTerm::Record { name, fields }
                    if name == "TextFile"
                        && fields[1].1 == NativeBoundaryTerm::Text("present".to_string())
            )
        )
    ));

    let failure = dispatch_vm_capability(&request(vec![
        NativeBoundaryTerm::Text(present.to_string_lossy().into_owned()),
        NativeBoundaryTerm::Text(missing.to_string_lossy().into_owned()),
    ]))
    .expect("typed batch failure");
    let NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Record { fields, .. }) = failure else {
        panic!("expected typed result failure, found {failure:?}");
    };
    let NativeBoundaryTerm::Record { fields: error, .. } = &fields[0].1 else {
        panic!("expected typed file error, found {:?}", fields[0].1);
    };
    assert_eq!(
        error[2].1,
        NativeBoundaryTerm::Text(missing.to_string_lossy().into_owned())
    );
}

#[test]
fn vm_owned_argument_capabilities_read_only_the_application_context() {
    let count = PureNativeCapabilityRequest {
        capability: "system.arguments".to_string(),
        operation: "std.system.arguments.count".to_string(),
        arguments: vec![],
        package_arguments: None,
        result_type: TvmBoundaryType::Int,
    };
    let value = PureNativeCapabilityRequest {
        capability: "system.arguments".to_string(),
        operation: "std.system.arguments.get".to_string(),
        arguments: vec![NativeBoundaryTerm::Int(1)],
        package_arguments: None,
        result_type: TvmBoundaryType::Managed(
            crate::runtime::native_image::managed::SemanticTypeId::from_canonical(
                "Apply(Option;String)",
            )
            .expect("Option[String] identity")
            .bytes(),
        ),
    };
    let arguments = ["input.tsv".to_string(), "--strict".to_string()];

    assert_eq!(
        dispatch_vm_capability_with_program_arguments(&count, &arguments),
        Ok(NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Int(2)))
    );
    assert_eq!(
        dispatch_vm_capability_with_program_arguments(&value, &arguments),
        Ok(NativeBoundaryReplyTerm::Ok(
            NativeBoundaryTerm::OptionalText(Some("--strict".to_string()))
        ))
    );
}

#[test]
fn platform_metrics_use_the_vm_capability_path_not_direct_std() {
    assert!(!super::direct_std::supports(
        "std.system.platform.current_metrics"
    ));
    let request = PureNativeCapabilityRequest {
        capability: "system.platform".to_string(),
        operation: "std.system.platform.current_metrics".to_string(),
        arguments: vec![],
        package_arguments: None,
        result_type: TvmBoundaryType::Managed(
            crate::runtime::native_image::managed::SemanticTypeId::from_canonical(
                "Named(std.system.Platform.HostMetrics)",
            )
            .expect("HostMetrics identity")
            .bytes(),
        ),
    };

    let reply = dispatch_vm_capability(&request).expect("VM-owned platform metrics");
    let NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Record { name, fields }) = reply else {
        panic!("expected HostMetrics record");
    };
    assert_eq!(name, "HostMetrics");
    assert_eq!(fields.len(), 12);
}

#[test]
fn package_helper_routes_canonical_namespaces_to_distinct_environment_bindings() {
    assert_eq!(
        package_operation_namespace("polars.dataframe.read_csv"),
        Ok("polars")
    );
    assert_eq!(
        package_helper_environment("polars"),
        Ok("TERLAN_POLARS_NATIVE_BOUNDARY_HELPER_PATH".to_string())
    );
    assert_eq!(
        package_helper_environment("pytorch"),
        Ok("TERLAN_PYTORCH_NATIVE_BOUNDARY_HELPER_PATH".to_string())
    );
    assert_eq!(
        helper_environment_namespace("TERLAN_PYTORCH_NATIVE_BOUNDARY_HELPER_PATH"),
        Ok("pytorch".to_string())
    );
}

#[test]
fn package_helper_rejects_missing_or_noncanonical_namespaces() {
    for operation in ["read_csv", "Polars.read_csv", "polars-native.read_csv"] {
        assert!(
            package_operation_namespace(operation)
                .expect_err("invalid package namespace must fail")
                .contains("error[native_helper_namespace]"),
            "{operation}"
        );
    }
}
