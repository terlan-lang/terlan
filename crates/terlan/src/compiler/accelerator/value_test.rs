use super::*;

fn principal(value: &str) -> AcceleratorResourcePrincipal {
    AcceleratorResourcePrincipal::new(value).expect("principal")
}

fn device(ordinal: u32) -> AcceleratorDeviceId {
    AcceleratorDeviceId::new("synthetic", ordinal).expect("device")
}

fn layout() -> AcceleratorTensorLayout {
    AcceleratorTensorLayout::new(
        AcceleratorScalarType::F32,
        &[2, 3],
        None,
        0,
        AcceleratorTensorOrder::RowMajor,
        4,
    )
    .expect("layout")
}

#[test]
fn scalar_model_has_stable_names_widths_and_rejections() {
    for (name, width) in [
        ("bool", 1),
        ("u8", 1),
        ("i8", 1),
        ("u16", 2),
        ("i16", 2),
        ("u32", 4),
        ("i32", 4),
        ("u64", 8),
        ("i64", 8),
        ("f16", 2),
        ("bf16", 2),
        ("f32", 4),
        ("f64", 8),
    ] {
        let dtype = AcceleratorScalarType::try_from(name).expect("known dtype");
        assert_eq!(dtype.identifier(), name);
        assert_eq!(dtype.byte_width(), width);
        assert_eq!(dtype.alignment(), width);
    }
    assert!(matches!(
        AcceleratorScalarType::try_from("complex128"),
        Err(AcceleratorValueError::UnsupportedScalarType(value)) if value == "complex128"
    ));
}

#[test]
fn tensor_layout_checks_order_sizes_offsets_and_strides() {
    let row = layout();
    assert_eq!(row.rank(), 2);
    assert_eq!(row.dimensions, [2, 3]);
    assert_eq!(row.strides, [3, 1]);
    assert_eq!(row.element_count, 6);
    assert_eq!(row.byte_size, 24);
    assert_eq!(row.storage_span_bytes, 24);
    assert!(row.is_row_major_contiguous());

    let column = AcceleratorTensorLayout::new(
        AcceleratorScalarType::F64,
        &[2, 3],
        Some(&[1, 2]),
        16,
        AcceleratorTensorOrder::ColumnMajor,
        8,
    )
    .expect("column-major");
    assert_eq!(column.strides, [1, 2]);
    assert!(!column.is_row_major_contiguous());

    let strided = AcceleratorTensorLayout::new(
        AcceleratorScalarType::U8,
        &[2, 2],
        Some(&[4, 1]),
        0,
        AcceleratorTensorOrder::Strided,
        1,
    )
    .expect("strided");
    assert_eq!(strided.byte_size, 4);
    assert_eq!(strided.storage_span_bytes, 6);

    let broadcast = AcceleratorTensorLayout::new(
        AcceleratorScalarType::F32,
        &[2, 3],
        Some(&[0, 1]),
        0,
        AcceleratorTensorOrder::Strided,
        4,
    )
    .expect("broadcast view");
    assert_eq!(broadcast.byte_size, 24);
    assert_eq!(broadcast.storage_span_bytes, 12);
    assert!(broadcast.is_broadcast_view());
    assert!(!broadcast.is_row_major_contiguous());
    assert!(broadcast.validate().is_ok());

    let empty = AcceleratorTensorLayout::new(
        AcceleratorScalarType::I32,
        &[4, 0, 8],
        None,
        0,
        AcceleratorTensorOrder::RowMajor,
        4,
    )
    .expect("empty");
    assert_eq!(empty.element_count, 0);
    assert_eq!(empty.storage_span_bytes, 0);
    assert!(empty.validate().is_ok());
}

#[test]
fn tensor_layout_rejects_every_unchecked_metadata_class() {
    assert!(matches!(
        AcceleratorTensorLayout::new(
            AcceleratorScalarType::F32,
            &[-1],
            None,
            0,
            AcceleratorTensorOrder::RowMajor,
            4
        ),
        Err(AcceleratorValueError::NegativeDimension(-1))
    ));
    assert!(matches!(
        AcceleratorTensorLayout::new(
            AcceleratorScalarType::F32,
            &vec![1; MAX_ACCELERATOR_TENSOR_RANK + 1],
            None,
            0,
            AcceleratorTensorOrder::RowMajor,
            4
        ),
        Err(AcceleratorValueError::InvalidRank(_))
    ));
    for strides in [vec![1], vec![3, -1]] {
        assert!(AcceleratorTensorLayout::new(
            AcceleratorScalarType::F32,
            &[2, 3],
            Some(&strides),
            0,
            AcceleratorTensorOrder::Strided,
            4
        )
        .is_err());
    }
    assert!(matches!(
        AcceleratorTensorLayout::new(
            AcceleratorScalarType::F32,
            &[2, 3],
            Some(&[1, 2]),
            0,
            AcceleratorTensorOrder::RowMajor,
            4
        ),
        Err(AcceleratorValueError::IncompatibleLayout)
    ));
    for alignment in [0, 2, 3] {
        assert!(matches!(
            AcceleratorTensorLayout::new(
                AcceleratorScalarType::F32,
                &[1],
                None,
                0,
                AcceleratorTensorOrder::RowMajor,
                alignment
            ),
            Err(AcceleratorValueError::InvalidAlignment(_))
        ));
    }
    assert!(matches!(
        AcceleratorTensorLayout::new(
            AcceleratorScalarType::F32,
            &[1],
            None,
            4,
            AcceleratorTensorOrder::RowMajor,
            8
        ),
        Err(AcceleratorValueError::MisalignedOffset { .. })
    ));
    assert!(matches!(
        AcceleratorTensorLayout::new(
            AcceleratorScalarType::F64,
            &[i64::MAX, i64::MAX],
            None,
            0,
            AcceleratorTensorOrder::RowMajor,
            8
        ),
        Err(AcceleratorValueError::IntegerOverflow(_))
    ));
}

#[test]
fn address_spaces_are_pointer_free_and_validate_identities() {
    let accelerator = device(2);
    let space = AcceleratorAddressSpace::Device {
        device: accelerator.clone(),
    };
    assert_eq!(space.device(), Some(&accelerator));
    assert!(AcceleratorAddressSpace::Host.validate().is_ok());
    assert!(AcceleratorAddressSpace::PinnedHost {
        backend: "synthetic".to_string()
    }
    .validate()
    .is_ok());
    assert!(AcceleratorAddressSpace::External {
        provider: "bad provider".to_string(),
        space: "shared".to_string()
    }
    .validate()
    .is_err());
    assert!(AcceleratorDeviceId::new("Bad", 0).is_err());
}

#[test]
fn linear_resource_enforces_borrow_transfer_staleness_and_one_deleter() {
    let owner = principal("actor.owner");
    let recipient = principal("actor.recipient");
    let borrower = principal("actor.borrower");
    let (mut resource, owned) = AcceleratorLinearResource::new(
        7,
        AcceleratorResourceClass::Allocation,
        AcceleratorAddressSpace::Device { device: device(0) },
        owner,
        AcceleratorDeleter::PackageOperation {
            package: "synthetic".to_string(),
            operation: "buffer.dispose".to_string(),
        },
    )
    .expect("resource");
    assert!(matches!(
        resource.borrow(&owned, borrower.clone(), 0),
        Err(AcceleratorValueError::EscapedBorrow)
    ));
    let borrowed = resource.borrow(&owned, borrower, 9).expect("borrow");
    assert!(matches!(
        resource.transfer(&owned, recipient.clone()),
        Err(AcceleratorValueError::BorrowActive)
    ));
    assert!(matches!(
        resource.dispose(&owned),
        Err(AcceleratorValueError::BorrowActive)
    ));
    resource.release_borrow(&borrowed).expect("release");
    let transferred = resource.transfer(&owned, recipient).expect("transfer");
    assert!(matches!(
        resource.validate_handle(&owned),
        Err(AcceleratorValueError::StaleHandle)
    ));
    let invocation = resource
        .dispose(&transferred)
        .expect("dispose")
        .expect("deleter");
    assert_eq!(invocation.resource.generation, 1);
    assert!(matches!(
        resource.dispose(&transferred),
        Err(AcceleratorValueError::AlreadyDisposed)
    ));
}

#[test]
fn tensor_packets_validate_copy_transfer_borrow_device_and_sizes() {
    let host = AcceleratorTensorPacket::new(AcceleratorTensorPacketInput {
        layout: layout(),
        address_space: AcceleratorAddressSpace::Host,
        device: None,
        stream: None,
        ownership: AcceleratorPacketOwnership::Copied,
        resource: None,
        deleter: AcceleratorDeleter::None,
        available_bytes: 24,
    })
    .expect("copied packet");
    assert_eq!(host.dtype(), AcceleratorScalarType::F32);
    assert!(host.validate_supported_dtypes(&["f32".to_string()]).is_ok());
    assert!(host.validate_supported_dtypes(&["u8".to_string()]).is_err());

    let owner = principal("actor.owner");
    let (mut resource, owned) = AcceleratorLinearResource::new(
        8,
        AcceleratorResourceClass::ImportedTensor,
        AcceleratorAddressSpace::Device { device: device(0) },
        owner.clone(),
        AcceleratorDeleter::None,
    )
    .expect("resource");
    let transferred = resource
        .transfer(&owned, principal("actor.receiver"))
        .expect("transfer");
    assert!(AcceleratorTensorPacket::new(AcceleratorTensorPacketInput {
        layout: layout(),
        address_space: AcceleratorAddressSpace::Device { device: device(0) },
        device: Some(device(0)),
        stream: None,
        ownership: AcceleratorPacketOwnership::Transferred,
        resource: Some(transferred.clone()),
        deleter: AcceleratorDeleter::None,
        available_bytes: 24,
    })
    .is_ok());

    let borrowed = resource
        .borrow(&transferred, principal("actor.borrower"), 4)
        .expect("borrow");
    assert!(AcceleratorTensorPacket::new(AcceleratorTensorPacketInput {
        layout: layout(),
        address_space: AcceleratorAddressSpace::Device { device: device(0) },
        device: Some(device(0)),
        stream: None,
        ownership: AcceleratorPacketOwnership::Borrowed { scope: 4 },
        resource: Some(borrowed.clone()),
        deleter: AcceleratorDeleter::None,
        available_bytes: 24,
    })
    .is_ok());
    resource.release_borrow(&borrowed).expect("release");
}

#[test]
fn tensor_packets_reject_adversarial_metadata_before_dispatch() {
    let too_small = AcceleratorTensorPacket::new(AcceleratorTensorPacketInput {
        layout: layout(),
        address_space: AcceleratorAddressSpace::Host,
        device: None,
        stream: None,
        ownership: AcceleratorPacketOwnership::Copied,
        resource: None,
        deleter: AcceleratorDeleter::None,
        available_bytes: 23,
    });
    assert!(matches!(
        too_small,
        Err(AcceleratorValueError::ByteCountMismatch { .. })
    ));
    assert!(matches!(
        AcceleratorTensorPacket::new(AcceleratorTensorPacketInput {
            layout: layout(),
            address_space: AcceleratorAddressSpace::Device { device: device(0) },
            device: Some(device(1)),
            stream: None,
            ownership: AcceleratorPacketOwnership::Copied,
            resource: None,
            deleter: AcceleratorDeleter::None,
            available_bytes: 24,
        }),
        Err(AcceleratorValueError::CrossDeviceAlias)
    ));
    assert!(matches!(
        AcceleratorTensorPacket::new(AcceleratorTensorPacketInput {
            layout: layout(),
            address_space: AcceleratorAddressSpace::Host,
            device: None,
            stream: None,
            ownership: AcceleratorPacketOwnership::Borrowed { scope: 0 },
            resource: None,
            deleter: AcceleratorDeleter::None,
            available_bytes: 24,
        }),
        Err(AcceleratorValueError::EscapedBorrow)
    ));

    let mut packet = AcceleratorTensorPacket::new(AcceleratorTensorPacketInput {
        layout: layout(),
        address_space: AcceleratorAddressSpace::Host,
        device: None,
        stream: None,
        ownership: AcceleratorPacketOwnership::Copied,
        resource: None,
        deleter: AcceleratorDeleter::None,
        available_bytes: 24,
    })
    .expect("packet");
    packet.schema = 99;
    assert!(matches!(
        packet.validate(),
        Err(AcceleratorValueError::UnsupportedPacketSchema(99))
    ));

    let mut forged = layout();
    forged.byte_size = 1;
    assert!(matches!(
        forged.validate(),
        Err(AcceleratorValueError::IncompatibleLayout)
    ));
}

#[test]
fn canonical_schema_drives_reports_and_package_declarations() {
    let contract = AcceleratorValueContract::canonical();
    assert_eq!(contract.schema, "terlan.accelerator-value-contract.v1");
    assert_eq!(contract.tensor_packet_schema, 1);
    assert_eq!(contract.scalar_types.len(), 13);
    assert_eq!(contract.ownership_transitions.len(), 4);
    assert!(contract
        .generated_adapters
        .iter()
        .any(|adapter| adapter.id == "rust-serde-codec-v1"));
    let declarations = contract.render_terlan_declarations("generated.AcceleratorValue");
    assert!(declarations.contains("module generated.AcceleratorValue."));
    assert!(declarations.contains("pub type DType ="));
    assert!(declarations.contains("Atom[\"bf16\"]"));
    assert!(declarations.contains("pub struct TensorPacket"));
    assert!(!declarations.contains("pointer"));

    let filtered = contract
        .render_terlan_declarations_for(
            "cuda.AcceleratorValue",
            &["f32".to_string(), "u8".to_string()],
        )
        .expect("filtered declarations");
    assert!(filtered.contains("Atom[\"f32\"]"));
    assert!(filtered.contains("Atom[\"u8\"]"));
    assert!(!filtered.contains("Atom[\"f64\"]"));
    assert!(contract
        .render_terlan_declarations_for("cuda.AcceleratorValue", &["unsupported".to_string()])
        .is_err());
    let rust = contract
        .render_rust_scalar_codec(&["bool".to_string(), "f32".to_string()])
        .expect("Rust codec");
    assert!(rust.contains("pub enum DType"));
    assert!(rust.contains("F32"));
    assert!(rust.contains("Self::Bool => (6, 8)"));
    assert!(rust.contains("pub struct Shape"));
    assert!(rust.contains("pub enum ShapeError"));
    assert!(rust.contains("pub strides: Vec<usize>"));
    assert!(rust.contains("pub fn encode_view_prefix("));
    assert!(rust.contains("pub fn is_broadcast_view(&self)"));
    assert!(rust.contains("pub struct ResourceHandle"));
    assert!(rust.contains("pub enum ResourceAccess"));
    assert!(rust.contains("pub fn borrow("));
    assert!(rust.contains("pub fn transfer("));
    assert!(!rust.contains("F64"));
}
