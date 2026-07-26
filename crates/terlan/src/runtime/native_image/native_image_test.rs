#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::process::Command;
#[cfg(target_os = "linux")]
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

#[cfg(target_os = "linux")]
use super::image::native_section_digests;
use super::image::validate_host_target_identity;
use super::managed::{
    encode_aggregate_layout, encode_collection_layout, ManagedAggregateDescriptor,
    ManagedCollectionDescriptor, ManagedFieldType,
};
use super::*;

#[test]
fn typed_transition_boundary_headers_round_trip_without_losing_semantic_identity() {
    let types = [
        TvmBoundaryType::Unit,
        TvmBoundaryType::Bool,
        TvmBoundaryType::Int,
        TvmBoundaryType::Float,
        TvmBoundaryType::Binary,
        TvmBoundaryType::String,
        TvmBoundaryType::Json,
        TvmBoundaryType::NativeResource(u64::MAX),
        TvmBoundaryType::Atom,
        TvmBoundaryType::Bytes,
        TvmBoundaryType::Managed([0xA5; 16]),
    ];
    for boundary_type in types {
        let words = boundary_type.transition_words();
        assert_eq!(
            TvmBoundaryType::from_transition_words(&words).expect("valid transition header"),
            boundary_type
        );
    }
}

#[test]
fn typed_transition_boundary_headers_reject_ambiguous_or_malformed_types() {
    assert!(TvmBoundaryType::from_transition_words(&[5, 1, 0])
        .expect_err("String metadata must be canonical")
        .contains("nonzero identity"));
    assert!(TvmBoundaryType::from_transition_words(&[7, 1, 1])
        .expect_err("resource metadata has one identity word")
        .contains("nonzero high"));
    assert!(TvmBoundaryType::from_transition_words(&[99, 0, 0])
        .expect_err("unknown tags must fail")
        .contains("unknown type tag"));
    assert!(TvmBoundaryType::from_transition_words(&[5, 0])
        .expect_err("truncated metadata must fail")
        .contains("expected 3"));
}

#[test]
fn control_protocol_round_trips_and_rejects_malformed_frames() {
    use super::control::{
        read_control_frame, write_control_frame, TvmControlFrame, TvmTransitionOperation,
    };

    let mut frames = vec![
        TvmControlFrame::Call {
            request_id: 7,
            owner_id: 5,
            export_id: 11,
            arguments: vec![13, -17],
        },
        TvmControlFrame::Success {
            request_id: 7,
            owner_id: 5,
            value: 13,
        },
        TvmControlFrame::Failure {
            request_id: 7,
            owner_id: 5,
            status: 4,
        },
        TvmControlFrame::Resume {
            request_id: 7,
            owner_id: 5,
            continuation_id: 19,
            values: vec![29],
        },
    ];
    for operation in [
        TvmTransitionOperation::Yield,
        TvmTransitionOperation::Send,
        TvmTransitionOperation::Receive,
        TvmTransitionOperation::Spawn,
        TvmTransitionOperation::Timer,
        TvmTransitionOperation::Link,
        TvmTransitionOperation::Monitor,
        TvmTransitionOperation::Resource,
        TvmTransitionOperation::Cancellation,
        TvmTransitionOperation::Failure,
        TvmTransitionOperation::Scheduling,
    ] {
        frames.push(TvmControlFrame::Transition {
            request_id: 7,
            owner_id: 5,
            continuation_id: 19,
            operation,
            arguments: vec![29, 31],
            values: vec![23],
        });
    }
    let mut encoded = Vec::new();
    for expected in frames {
        encoded.clear();
        write_control_frame(&mut encoded, &expected).expect("encode control frame");
        assert_eq!(
            read_control_frame(&mut encoded.as_slice()).expect("decode control frame"),
            Some(expected)
        );
    }

    let mut bad_magic = encoded.clone();
    bad_magic[0] = b'X';
    assert!(read_control_frame(&mut bad_magic.as_slice())
        .expect_err("bad magic must fail")
        .contains("tvm.control.magic"));

    let mut oversized = Vec::from(&b"TVMC\x01\x00\x03\x00"[..]);
    oversized.extend_from_slice(&((super::control::MAX_FRAME_BYTES + 1) as u32).to_le_bytes());
    assert!(read_control_frame(&mut oversized.as_slice())
        .expect_err("oversized frame must fail")
        .contains("tvm.control.frame_size"));

    let malformed_success = b"TVMC\x01\x00\x04\x00\x01\x00\x00\x00\0";
    assert!(read_control_frame(&mut malformed_success.as_slice())
        .expect_err("malformed success payload must fail")
        .contains("tvm.control.payload"));

    let mut malformed_transition = Vec::new();
    write_control_frame(
        &mut malformed_transition,
        &TvmControlFrame::Transition {
            request_id: 7,
            owner_id: 5,
            continuation_id: 19,
            operation: TvmTransitionOperation::Yield,
            arguments: Vec::new(),
            values: Vec::new(),
        },
    )
    .expect("encode transition");
    malformed_transition[36..38].copy_from_slice(&99_u16.to_le_bytes());
    assert!(read_control_frame(&mut malformed_transition.as_slice())
        .expect_err("unknown transition operation must fail")
        .contains("tvm.control.transition"));

    let mut malformed_arguments = Vec::new();
    write_control_frame(
        &mut malformed_arguments,
        &TvmControlFrame::Transition {
            request_id: 7,
            owner_id: 5,
            continuation_id: 19,
            operation: TvmTransitionOperation::Send,
            arguments: Vec::new(),
            values: Vec::new(),
        },
    )
    .expect("encode transition argument envelope");
    malformed_arguments[38..40].copy_from_slice(&1_u16.to_le_bytes());
    assert!(read_control_frame(&mut malformed_arguments.as_slice())
        .expect_err("malformed transition argument count must fail")
        .contains("tvm.control.values"));

    let mut malformed_resume = Vec::new();
    write_control_frame(
        &mut malformed_resume,
        &TvmControlFrame::Resume {
            request_id: 7,
            owner_id: 5,
            continuation_id: 19,
            values: Vec::new(),
        },
    )
    .expect("encode resume");
    malformed_resume[36..38].copy_from_slice(&1_u16.to_le_bytes());
    assert!(read_control_frame(&mut malformed_resume.as_slice())
        .expect_err("nonzero resume reserved bits must fail")
        .contains("tvm.control.resume"));
}

fn descriptor() -> TvmExecutableDescriptor {
    TvmExecutableDescriptor {
        runtime_abi_min: 2,
        runtime_abi_max: 2,
        native_boundary_min: 1,
        native_boundary_max: 1,
        target: TvmImageTarget {
            triple: "native-test-target".to_string(),
            architecture: std::env::consts::ARCH.to_string(),
            operating_system: std::env::consts::OS.to_string(),
            calling_convention: "c".to_string(),
        },
        identity: TvmImageIdentity {
            compiler: "terlc-0.0.7".to_string(),
            build: "sha256:fixture".to_string(),
            package: "native-image-test".to_string(),
            module: "app.Main".to_string(),
        },
        exports: vec![
            TvmExportDescriptor {
                id: 10,
                name: "cleanup/1".to_string(),
                parameters: vec![TvmBoundaryType::NativeResource(30)],
                results: vec![TvmBoundaryType::Unit],
            },
            TvmExportDescriptor {
                id: 20,
                name: "add/2".to_string(),
                parameters: vec![TvmBoundaryType::Int, TvmBoundaryType::Int],
                results: vec![TvmBoundaryType::Int],
            },
            TvmExportDescriptor {
                id: 21,
                name: "managed/5".to_string(),
                parameters: vec![
                    TvmBoundaryType::Atom,
                    TvmBoundaryType::String,
                    TvmBoundaryType::Bytes,
                    TvmBoundaryType::Binary,
                    TvmBoundaryType::Managed([7; 16]),
                ],
                results: vec![TvmBoundaryType::String],
            },
        ],
        capabilities: vec![40],
        resources: vec![TvmNativeResourceDescriptor {
            type_id: 30,
            owner_capability_id: 40,
            cleanup_export_id: 10,
        }],
        dependencies: vec![TvmDependencyDescriptor {
            id: 50,
            abi_digest: [5; 32],
        }],
        continuations: vec![TvmContinuationDescriptor {
            id: 60,
            parameters: Vec::new(),
            results: vec![TvmBoundaryType::Bool],
        }],
        callables: vec![TvmCallableDescriptor {
            id: 20,
            parameters: vec![TvmBoundaryType::Int, TvmBoundaryType::Int],
            results: vec![TvmBoundaryType::Int],
            captures: Vec::new(),
        }],
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
        integrity: TvmImageIntegrity {
            code_digest: [0; 32],
            immutable_data_digest: [0; 32],
        },
        signature: Some(TvmSignatureDescriptor {
            signer: "test-signer".to_string(),
            signature: vec![7; 64],
        }),
    }
}

#[test]
fn macho_debug_metadata_uses_a_retained_terlan_section() {
    use object::write::Object as WriteObject;
    use object::{Architecture, BinaryFormat, Endianness, Object, ObjectSection, SectionKind};

    let native = WriteObject::new(
        BinaryFormat::MachO,
        Architecture::Aarch64,
        Endianness::Little,
    )
    .write()
    .expect("write Mach-O fixture");
    let embedded =
        descriptor_object_for_native_with_debug(&native, &descriptor(), b"debug metadata")
            .expect("embed Mach-O metadata");
    let object = object::File::parse(embedded.as_slice()).expect("parse embedded Mach-O object");
    let sections = object
        .sections()
        .filter(|section| section.name().ok() == Some("__terlan"))
        .collect::<Vec<_>>();
    assert_eq!(sections.len(), 1);
    assert_ne!(sections[0].kind(), SectionKind::Debug);
    assert_eq!(
        sections[0].segment_name().expect("Mach-O segment name"),
        Some("__TERLAN")
    );
    assert_eq!(
        sections[0].data().expect("Mach-O metadata"),
        b"debug metadata"
    );
}

#[test]
fn descriptor_round_trip_is_canonical_and_deterministic() {
    let expected = descriptor();
    let first = encode_descriptor(&expected).expect("encode descriptor");
    let second = encode_descriptor(&expected).expect("encode descriptor again");

    assert_eq!(first, second);
    assert_eq!(&first[..8], b"TVMDSC01");
    assert_eq!(
        decode_descriptor(&first).expect("decode descriptor"),
        expected
    );
}

/// Round-trips the finite atom table and rejects noncanonical or invalid identities.
#[test]
fn descriptor_validates_canonical_atom_table() {
    let mut expected = descriptor();
    expected.atoms = vec!["error".to_owned(), "ready".to_owned()];
    let encoded = encode_descriptor(&expected).expect("encode descriptor with atom table");
    assert_eq!(
        decode_descriptor(&encoded).expect("decode descriptor with atom table"),
        expected
    );

    for atoms in [
        vec!["ready".to_owned(), "error".to_owned()],
        vec!["ready".to_owned(), "ready".to_owned()],
    ] {
        let mut malformed = descriptor();
        malformed.atoms = atoms;
        assert!(encode_descriptor(&malformed)
            .expect_err("noncanonical atom table")
            .contains("atom_order"));
    }

    for atom in ["", "bad\0atom", "bad\natom"] {
        let mut malformed = descriptor();
        malformed.atoms = vec![atom.to_owned()];
        assert!(encode_descriptor(&malformed)
            .expect_err("invalid atom identity")
            .contains("atoms"));
    }
}

/// Round-trips admitted layouts and rejects duplicate, mismatched, or malformed rows.
#[test]
fn descriptor_validates_canonical_managed_layout_registry() {
    let aggregate = ManagedAggregateDescriptor::tuple(
        "test.Pair",
        vec![ManagedFieldType::Int, ManagedFieldType::Bool],
    )
    .expect("aggregate layout");
    let layout = admitted_managed_layout(aggregate);
    let mut expected = descriptor();
    expected.managed_layouts.push(layout.clone());
    let encoded = encode_descriptor(&expected).expect("encode descriptor with managed layout");
    assert_eq!(
        decode_descriptor(&encoded).expect("decode descriptor with managed layout"),
        expected
    );

    let mut duplicate = descriptor();
    duplicate.managed_layouts = vec![layout.clone(), layout.clone()];
    assert!(encode_descriptor(&duplicate)
        .expect_err("duplicate layouts must fail")
        .contains("managed_layout_order"));

    let mut mismatch = descriptor();
    mismatch.managed_layouts = vec![TvmManagedLayoutDescriptor {
        semantic_id: [9; 16],
        encoded_layout: layout.encoded_layout.clone(),
    }];
    assert!(encode_descriptor(&mismatch)
        .expect_err("mismatched semantic identity must fail")
        .contains("managed_layout_identity"));

    let mut malformed = descriptor();
    malformed.managed_layouts = vec![TvmManagedLayoutDescriptor {
        semantic_id: layout.semantic_id,
        encoded_layout: b"not-a-layout".to_vec(),
    }];
    assert!(encode_descriptor(&malformed)
        .expect_err("malformed aggregate bytes must fail")
        .contains("managed_layout"));

    let duplicate_variant = ManagedAggregateDescriptor::constructor(
        "test.Result",
        "Ok",
        1,
        2,
        vec![(Some("value".to_string()), ManagedFieldType::Int)],
    )
    .expect("duplicate constructor variant");
    let original_variant = ManagedAggregateDescriptor::constructor(
        "test.Result",
        "Ok",
        0,
        2,
        vec![(Some("value".to_string()), ManagedFieldType::Int)],
    )
    .expect("original constructor variant");
    let mut variants = vec![
        admitted_managed_layout(original_variant),
        admitted_managed_layout(duplicate_variant),
    ];
    variants.sort_by(|left, right| left.encoded_layout.cmp(&right.encoded_layout));
    let mut duplicate_variant = descriptor();
    duplicate_variant.managed_layouts = variants;
    assert!(encode_descriptor(&duplicate_variant)
        .expect_err("duplicate variant names must fail")
        .contains("managed_layout_variant"));
}

/// Round-trips collection schemas and rejects duplicate, mismatched, or malformed rows.
#[test]
fn descriptor_validates_canonical_managed_collection_registry() {
    let collection = ManagedCollectionDescriptor::list("List(Int)", ManagedFieldType::Int)
        .expect("collection schema");
    let layout = admitted_managed_collection(collection);
    let mut expected = descriptor();
    expected.managed_collections.push(layout.clone());
    let encoded = encode_descriptor(&expected).expect("encode descriptor with collection schema");
    assert_eq!(
        decode_descriptor(&encoded).expect("decode descriptor with collection schema"),
        expected
    );

    let mut duplicate = descriptor();
    duplicate.managed_collections = vec![layout.clone(), layout.clone()];
    assert!(encode_descriptor(&duplicate)
        .expect_err("duplicate collection schemas must fail")
        .contains("managed_collection_order"));

    let mut mismatch = descriptor();
    mismatch.managed_collections = vec![TvmManagedCollectionDescriptor {
        semantic_id: [9; 16],
        encoded_layout: layout.encoded_layout.clone(),
    }];
    assert!(encode_descriptor(&mismatch)
        .expect_err("mismatched collection identity must fail")
        .contains("managed_collection_identity"));

    let mut malformed = descriptor();
    malformed.managed_collections = vec![TvmManagedCollectionDescriptor {
        semantic_id: layout.semantic_id,
        encoded_layout: b"not-a-collection".to_vec(),
    }];
    assert!(encode_descriptor(&malformed)
        .expect_err("malformed collection bytes must fail")
        .contains("managed_collection"));
}

/// Builds one admitted managed-layout row for descriptor validation tests.
fn admitted_managed_layout(aggregate: ManagedAggregateDescriptor) -> TvmManagedLayoutDescriptor {
    TvmManagedLayoutDescriptor {
        semantic_id: aggregate.managed().semantic_id().bytes(),
        encoded_layout: encode_aggregate_layout(&aggregate).expect("encode aggregate layout"),
    }
}

/// Builds one admitted managed-collection row for descriptor validation tests.
fn admitted_managed_collection(
    collection: ManagedCollectionDescriptor,
) -> TvmManagedCollectionDescriptor {
    TvmManagedCollectionDescriptor {
        semantic_id: collection.semantic_id().bytes(),
        encoded_layout: encode_collection_layout(&collection).expect("encode collection schema"),
    }
}

#[test]
fn descriptor_rejects_tampering_and_noncanonical_records() {
    let encoded = encode_descriptor(&descriptor()).expect("encode descriptor");
    let mut tampered = encoded.clone();
    tampered[40] ^= 1;
    assert_error(&tampered, "descriptor_digest");

    let mut reordered = encoded;
    reordered[32..34].copy_from_slice(&2_u16.to_le_bytes());
    resign(&mut reordered);
    assert_error(&reordered, "record_order");
}

#[test]
fn descriptor_rejects_invalid_abi_ids_and_boundary_types() {
    let mut invalid = descriptor();
    invalid.runtime_abi_min = 2;
    invalid.runtime_abi_max = 1;
    assert!(encode_descriptor(&invalid)
        .expect_err("invalid ABI must fail")
        .contains("runtime_abi"));

    let mut duplicate = descriptor();
    duplicate.exports[1].id = duplicate.exports[0].id;
    assert!(encode_descriptor(&duplicate)
        .expect_err("duplicate export must fail")
        .contains("export_order"));

    let mut undeclared = descriptor();
    undeclared.exports[1].parameters[0] = TvmBoundaryType::NativeResource(999);
    assert!(encode_descriptor(&undeclared)
        .expect_err("undeclared resource must fail")
        .contains("resource_reference"));

    let mut unknown_type = encode_descriptor(&descriptor()).expect("encode descriptor");
    let exports = record_payload(&unknown_type, 3);
    let first_parameter_tag = exports + 2 + 8 + 2 + "cleanup/1".len() + 2;
    unknown_type[first_parameter_tag] = u8::MAX;
    resign(&mut unknown_type);
    assert_error(&unknown_type, "boundary_type");

    let mut duplicate_continuation = descriptor();
    duplicate_continuation
        .continuations
        .push(duplicate_continuation.continuations[0].clone());
    assert!(encode_descriptor(&duplicate_continuation)
        .expect_err("duplicate continuation must fail")
        .contains("continuation_order"));

    let mut colliding_continuation = descriptor();
    colliding_continuation.continuations[0].id = colliding_continuation.exports[0].id;
    assert!(encode_descriptor(&colliding_continuation)
        .expect_err("export/continuation collision must fail")
        .contains("continuation_collision"));

    let mut undeclared_continuation_resource = descriptor();
    undeclared_continuation_resource.continuations[0].parameters =
        vec![TvmBoundaryType::NativeResource(999)];
    assert!(encode_descriptor(&undeclared_continuation_resource)
        .expect_err("undeclared continuation resource must fail")
        .contains("resource_reference"));

    let mut wrong_callable_signature = descriptor();
    wrong_callable_signature.callables[0].parameters.pop();
    assert!(encode_descriptor(&wrong_callable_signature)
        .expect_err("callable/export signature drift must fail")
        .contains("callable_export_signature"));

    let mut untraced_callable = descriptor();
    untraced_callable.callables[0].captures = vec![TvmBoundaryType::Json];
    assert!(encode_descriptor(&untraced_callable)
        .expect_err("untraced callable capture must fail")
        .contains("callable_json"));

    let mut colliding_callable = descriptor();
    colliding_callable.callables[0].id = colliding_callable.continuations[0].id;
    assert!(encode_descriptor(&colliding_callable)
        .expect_err("callable/continuation collision must fail")
        .contains("callable_collision"));
}

#[test]
fn descriptor_rejects_malformed_ownership_dependency_and_signature_tables() {
    let mut missing_owner = descriptor();
    missing_owner.resources[0].owner_capability_id = 999;
    assert!(encode_descriptor(&missing_owner)
        .expect_err("missing owner capability must fail")
        .contains("resource_capability"));

    let mut missing_cleanup = descriptor();
    missing_cleanup.resources[0].cleanup_export_id = 999;
    assert!(encode_descriptor(&missing_cleanup)
        .expect_err("missing cleanup export must fail")
        .contains("resource_cleanup"));

    let mut duplicate_dependency = descriptor();
    duplicate_dependency
        .dependencies
        .push(TvmDependencyDescriptor {
            id: 50,
            abi_digest: [6; 32],
        });
    assert!(encode_descriptor(&duplicate_dependency)
        .expect_err("duplicate dependency must fail")
        .contains("dependency_order"));

    let mut empty_signature = descriptor();
    empty_signature
        .signature
        .as_mut()
        .unwrap()
        .signature
        .clear();
    assert!(encode_descriptor(&empty_signature)
        .expect_err("empty signature must fail")
        .contains("tvm.image.signature"));
}

#[test]
fn native_inspection_rejects_json_and_non_executables() {
    assert!(
        inspect_tvm_image(br#"{"format":"tvm"}"#, "native-test-target")
            .expect_err("JSON must fail")
            .contains("JSON is not a TVM image")
    );
    assert!(
        inspect_tvm_image(b"TVMIR\0compiler-data", "native-test-target")
            .expect_err("compiler IR must fail")
            .contains("native_format")
    );
}

#[test]
fn host_target_identity_rejects_each_independent_abi_dimension() {
    let host = host_tvm_target().expect("host target");
    validate_host_target_identity(&host, &host).expect("exact host identity");

    for (field, candidate) in [
        (
            "architecture",
            TvmImageTarget {
                architecture: "forged-architecture".to_string(),
                ..host.clone()
            },
        ),
        (
            "operating_system",
            TvmImageTarget {
                operating_system: "forged-operating-system".to_string(),
                ..host.clone()
            },
        ),
        (
            "calling_convention",
            TvmImageTarget {
                calling_convention: "forged-calling-convention".to_string(),
                ..host.clone()
            },
        ),
    ] {
        let error = validate_host_target_identity(&candidate, &host)
            .expect_err("forged host ABI dimension must fail");
        assert!(error.contains(field), "unexpected error: {error}");
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn coff_metadata_survives_pe_linking_sealing_and_inspection() {
    use object::write::{Object as WriteObject, Symbol, SymbolSection};
    use object::{
        Architecture, BinaryFormat, Endianness, SectionKind, SymbolFlags, SymbolKind, SymbolScope,
    };

    use super::debug::{encode_tvm_native_debug, inspect_tvm_native_debug, TvmNativeDebugRecord};

    let fixture = TestFiles::new("pe");
    let native_path = fixture.root.join("native.obj");
    let descriptor_path = fixture.root.join("descriptor.obj");
    let image_path = fixture.root.join("linked.exe");

    let mut native = WriteObject::new(BinaryFormat::Coff, Architecture::X86_64, Endianness::Little);
    let text = native.add_section(Vec::new(), b".text".to_vec(), SectionKind::Text);
    native.section_mut(text).set_data(vec![0xc3], 1);
    native.add_symbol(Symbol {
        name: TVM_IMAGE_ENTRY_SYMBOL_V1.as_bytes().to_vec(),
        value: 0,
        size: 1,
        kind: SymbolKind::Text,
        scope: SymbolScope::Linkage,
        weak: false,
        section: SymbolSection::Section(text),
        flags: SymbolFlags::None,
    });
    let native_bytes = native.write().expect("write COFF native object");
    fs::write(&native_path, &native_bytes).expect("write native object");

    let target = host_tvm_target().expect("host target");
    let mut expected = descriptor();
    expected.target = target.clone();
    let debug_record = TvmNativeDebugRecord {
        source_file: "Main.terl".to_string(),
        module: "app.Main".to_string(),
        function: "main".to_string(),
        arity: 0,
        span_start: 0,
        span_end: 1,
        core_schema: "core-ir-v1".to_string(),
        proof_readiness: "executable".to_string(),
    };
    let debug_bytes = encode_tvm_native_debug(std::slice::from_ref(&debug_record))
        .expect("encode debug metadata");
    let descriptor_bytes =
        descriptor_object_for_native_with_debug(&native_bytes, &expected, &debug_bytes)
            .expect("write COFF descriptor object");
    fs::write(&descriptor_path, descriptor_bytes).expect("write descriptor object");

    let linker = Path::new(
        std::str::from_utf8(
            &Command::new("rustc")
                .args(["--print", "sysroot"])
                .output()
                .expect("locate Rust sysroot")
                .stdout,
        )
        .expect("Rust sysroot is utf-8")
        .trim(),
    )
    .join("lib")
    .join("rustlib")
    .join(&target.triple)
    .join("bin")
    .join("rust-lld");
    let output = Command::new(&linker)
        .args([
            "-flavor",
            "link",
            "/subsystem:console",
            "/nodefaultlib",
            &format!("/entry:{TVM_IMAGE_ENTRY_SYMBOL_V1}"),
            &format!("/out:{}", image_path.display()),
        ])
        .arg(&native_path)
        .arg(&descriptor_path)
        .output()
        .expect("link PE fixture with rust-lld");
    assert!(
        output.status.success(),
        "rust-lld failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let mut image = fs::read(&image_path).expect("read linked PE fixture");
    let sealed = seal_tvm_image(&mut image, &expected).expect("seal linked PE fixture");
    let inspection = inspect_tvm_image(&image, &target.triple).expect("inspect linked PE fixture");
    assert_eq!(inspection.format, "pe");
    assert_eq!(inspection.descriptor_section, ".tvm");
    assert_eq!(inspection.descriptor, sealed);
    assert_eq!(
        inspect_tvm_native_debug(&image).expect("inspect linked PE debug metadata"),
        vec![debug_record]
    );
}

#[test]
fn lightweight_host_target_matches_cranelift_codegen_identity() {
    let actual = host_tvm_target().expect("lightweight host target");
    let builder = cranelift_native::builder().expect("Cranelift host target");
    let triple = builder.triple();
    let expected = TvmImageTarget {
        triple: triple.to_string(),
        architecture: triple.architecture.to_string(),
        operating_system: triple.operating_system.to_string(),
        calling_convention: cranelift_codegen::isa::CallConv::triple_default(triple).to_string(),
    };

    assert_eq!(actual, expected);
}

#[cfg(target_os = "linux")]
#[test]
fn native_inspection_accepts_real_elf_and_rejects_wrong_target_and_abi() {
    let fixture = TestFiles::new("elf");
    let executable = fixture.root.join("test-executable");
    fs::copy("/proc/self/exe", &executable).expect("copy running test executable");
    let original = fs::read(&executable).expect("read test executable");
    let (code_digest, immutable_data_digest) =
        native_section_digests(&original).expect("digest native sections");
    let mut valid = descriptor();
    valid.integrity = TvmImageIntegrity {
        code_digest,
        immutable_data_digest,
    };
    let image = fixture.embed(&executable, &valid);
    let image_bytes = fs::read(&image).expect("read image");

    let inspection =
        inspect_tvm_image(&image_bytes, "native-test-target").expect("inspect valid native image");
    assert_eq!(inspection.format, "elf");
    assert_eq!(inspection.descriptor_section, ".note.terlan.tvm");
    assert_eq!(inspection.descriptor.identity.module, "app.Main");
    assert!(inspect_tvm_image(&image_bytes, "wrong-target")
        .expect_err("wrong target must fail")
        .contains("image.target"));
    assert!(inspect_tvm_image(&original, "native-test-target")
        .expect_err("missing descriptor must fail")
        .contains("descriptor_section"));

    let mut bad_digest = descriptor();
    bad_digest.integrity = TvmImageIntegrity {
        code_digest: [9; 32],
        immutable_data_digest,
    };
    let bad_digest_image = fixture.embed_named(&executable, &bad_digest, "bad-digest");
    assert!(inspect_tvm_image(
        &fs::read(bad_digest_image).expect("read bad-digest image"),
        "native-test-target",
    )
    .expect_err("bad code digest must fail")
    .contains("code_digest"));

    valid.runtime_abi_min = 1;
    valid.runtime_abi_max = 1;
    let incompatible = fixture.embed_named(&executable, &valid, "incompatible");
    assert!(inspect_tvm_image(
        &fs::read(incompatible).expect("read incompatible image"),
        "native-test-target",
    )
    .expect_err("unsupported ABI must fail")
    .contains("runtime_abi"));

    valid.runtime_abi_min = 2;
    valid.runtime_abi_max = 2;
    valid.native_boundary_min = 2;
    valid.native_boundary_max = 2;
    let incompatible_adapter = fixture.embed_named(&executable, &valid, "incompatible-adapter");
    assert!(inspect_tvm_image(
        &fs::read(incompatible_adapter).expect("read incompatible adapter image"),
        "native-test-target",
    )
    .expect_err("unsupported public adapter ABI must fail")
    .contains("native_boundary"));
}

fn assert_error(bytes: &[u8], expected: &str) {
    let error = decode_descriptor(bytes).expect_err("descriptor must fail");
    assert!(error.contains(expected), "unexpected error: {error}");
}

fn resign(bytes: &mut [u8]) {
    let digest_offset = bytes.len() - 32;
    let digest = Sha256::digest(&bytes[..digest_offset]);
    bytes[digest_offset..].copy_from_slice(&digest);
}

fn record_payload(bytes: &[u8], wanted_kind: u16) -> usize {
    let count = u16::from_le_bytes([bytes[14], bytes[15]]) as usize;
    let mut offset = 32;
    for _ in 0..count {
        let kind = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        let len = u32::from_le_bytes(
            bytes[offset + 4..offset + 8]
                .try_into()
                .expect("record length"),
        ) as usize;
        if kind == wanted_kind {
            return offset + 8;
        }
        offset += 8 + len;
    }
    panic!("missing record {wanted_kind}");
}

#[cfg(target_os = "linux")]
struct TestFiles {
    root: PathBuf,
}

#[cfg(target_os = "linux")]
impl TestFiles {
    fn new(name: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "terlan-native-image-{name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create fixture directory");
        Self { root }
    }

    fn embed(&self, executable: &Path, descriptor: &TvmExecutableDescriptor) -> PathBuf {
        self.embed_named(executable, descriptor, "valid")
    }

    fn embed_named(
        &self,
        executable: &Path,
        descriptor: &TvmExecutableDescriptor,
        name: &str,
    ) -> PathBuf {
        let descriptor_path = self.root.join(format!("{name}.descriptor"));
        let image_path = self.root.join(format!("{name}.tvm"));
        fs::write(
            &descriptor_path,
            encode_descriptor(descriptor).expect("encode fixture descriptor"),
        )
        .expect("write descriptor");
        let status = Command::new("objcopy")
            .arg("--add-section")
            .arg(format!(".note.terlan.tvm={}", descriptor_path.display()))
            .arg("--set-section-flags")
            .arg(".note.terlan.tvm=readonly,data")
            .arg(executable)
            .arg(&image_path)
            .status()
            .expect("run objcopy; binutils is required by this gate");
        assert!(status.success(), "objcopy failed with {status}");
        image_path
    }
}

#[cfg(target_os = "linux")]
impl Drop for TestFiles {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
