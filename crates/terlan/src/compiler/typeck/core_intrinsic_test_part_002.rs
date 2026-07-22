
/// Verifies VM primitive stdlib calls resolve to CoreIR intrinsic ids.
///
/// Inputs:
/// - Source-level module/function/arity triples for `std.vm.Bytes`,
///   `std.vm.Timeout`, `std.vm.Tcp`, and `std.vm.Port`.
///
/// Output:
/// - Test passes when every selected operation maps to the expected closed
///   CoreIR primitive intrinsic.
///
/// Transformation:
/// - Exercises the typechecker intrinsic selector without relying on Vm
///   backend names, keeping VM primitive capability admission centralized in
///   CoreIR.
#[test]
fn core_primitive_intrinsic_resolves_vm_library_primitives() {
    let cases = [
        (
            "std.vm.Bytes",
            "from_list",
            1,
            CorePrimitiveIntrinsic::VmBytesFromList,
        ),
        (
            "std.vm.Bytes",
            "to_list",
            1,
            CorePrimitiveIntrinsic::VmBytesToList,
        ),
        (
            "std.vm.Bytes",
            "length",
            1,
            CorePrimitiveIntrinsic::VmBytesLength,
        ),
        (
            "std.vm.Bytes",
            "concat",
            2,
            CorePrimitiveIntrinsic::VmBytesConcat,
        ),
        (
            "std.vm.Bytes",
            "slice",
            3,
            CorePrimitiveIntrinsic::VmBytesSlice,
        ),
        (
            "std.vm.Bytes",
            "read_uint_be",
            3,
            CorePrimitiveIntrinsic::VmBytesReadUintBe,
        ),
        (
            "std.vm.Bytes",
            "read_int_be",
            3,
            CorePrimitiveIntrinsic::VmBytesReadIntBe,
        ),
        (
            "std.vm.Bytes",
            "read_uint_le",
            3,
            CorePrimitiveIntrinsic::VmBytesReadUintLe,
        ),
        (
            "std.vm.Bytes",
            "read_int_le",
            3,
            CorePrimitiveIntrinsic::VmBytesReadIntLe,
        ),
        (
            "std.vm.Timeout",
            "milliseconds",
            1,
            CorePrimitiveIntrinsic::VmTimeoutMilliseconds,
        ),
        (
            "std.vm.Timeout",
            "forever",
            0,
            CorePrimitiveIntrinsic::VmTimeoutForever,
        ),
        (
            "std.vm.Tcp",
            "listen",
            1,
            CorePrimitiveIntrinsic::VmTcpListen,
        ),
        (
            "std.vm.Tcp",
            "listen_with_backlog",
            2,
            CorePrimitiveIntrinsic::VmTcpListenWithBacklog,
        ),
        (
            "std.vm.Tcp",
            "accept",
            2,
            CorePrimitiveIntrinsic::VmTcpAccept,
        ),
        (
            "std.vm.Tcp",
            "connect",
            3,
            CorePrimitiveIntrinsic::VmTcpConnect,
        ),
        ("std.vm.Tcp", "send", 2, CorePrimitiveIntrinsic::VmTcpSend),
        (
            "std.vm.Tcp",
            "receive",
            3,
            CorePrimitiveIntrinsic::VmTcpReceive,
        ),
        ("std.vm.Tcp", "close", 1, CorePrimitiveIntrinsic::VmTcpClose),
        (
            "std.vm.Tcp",
            "close_listener",
            1,
            CorePrimitiveIntrinsic::VmTcpCloseListener,
        ),
        ("std.vm.Port", "open", 1, CorePrimitiveIntrinsic::VmPortOpen),
        (
            "std.vm.Port",
            "write",
            2,
            CorePrimitiveIntrinsic::VmPortWrite,
        ),
        ("std.vm.Port", "read", 3, CorePrimitiveIntrinsic::VmPortRead),
        (
            "std.vm.Port",
            "close",
            1,
            CorePrimitiveIntrinsic::VmPortClose,
        ),
    ];

    for (module, function, arity, expected) in cases {
        assert_eq!(
            core_primitive_intrinsic(module, function, arity),
            Some(expected),
            "{module}.{function}/{arity} should resolve"
        );
    }
}

/// Verifies persistent map take has one closed CoreIR contract.
///
/// Inputs:
/// - Canonical `std.collections.Map.take/2` and adjacent invalid arities.
///
/// Output:
/// - Test passes when only the valid call resolves to `MapTake`, its stable key
///   remains `core.map.take`, and its result is value-plus-remainder.
///
/// Transformation:
/// - Locks the compiler-owned map operation independently from the runtime's
///   imported-module alias adapter.
#[test]
fn core_primitive_intrinsic_resolves_persistent_map_take() {
    let intrinsic = core_primitive_intrinsic("std.collections.Map", "take", 2)
        .expect("canonical map take intrinsic");

    assert_eq!(intrinsic, CorePrimitiveIntrinsic::MapTake);
    assert_eq!(intrinsic.registry_key(), "core.map.take");
    assert_eq!(
        core_primitive_intrinsic_return_type(&intrinsic),
        CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(CoreType::Apply {
                constructor: "Option".to_string(),
                args: vec![CoreType::Named("Dynamic".to_string())],
            }),
            CoreTupleTypeElem::Type(CoreType::Named("Map".to_string())),
        ])
    );
    assert_eq!(
        core_primitive_intrinsic("std.collections.Map", "take", 1),
        None
    );
    assert_eq!(
        core_primitive_intrinsic("std.collections.Map", "take", 3),
        None
    );
}

/// Verifies VM primitive registry keys stay stable.
///
/// Inputs:
/// - Newly admitted VM primitive intrinsic variants.
///
/// Output:
/// - Test passes when each variant serializes to its expected backend-neutral
///   registry key.
///
/// Transformation:
/// - Guards CoreIR contract text against accidental key churn for socket and
///   port primitives that future Terlan integration tests will depend on.
#[test]
fn vm_library_primitive_registry_keys_are_stable() {
    for (intrinsic, key) in [
        (CorePrimitiveIntrinsic::VmAgentStart, "vm.agent.start"),
        (CorePrimitiveIntrinsic::VmAgentGet, "vm.agent.get"),
        (
            CorePrimitiveIntrinsic::VmAgentGetAndUpdate,
            "vm.agent.get_and_update",
        ),
        (CorePrimitiveIntrinsic::VmAgentUpdate, "vm.agent.update"),
        (CorePrimitiveIntrinsic::VmAgentCast, "vm.agent.cast"),
        (CorePrimitiveIntrinsic::VmAgentStop, "vm.agent.stop"),
        (
            CorePrimitiveIntrinsic::VmGenServerStart,
            "vm.gen_server.start",
        ),
        (
            CorePrimitiveIntrinsic::VmGenServerCall,
            "vm.gen_server.call",
        ),
        (
            CorePrimitiveIntrinsic::VmGenServerCast,
            "vm.gen_server.cast",
        ),
        (
            CorePrimitiveIntrinsic::VmGenServerStop,
            "vm.gen_server.stop",
        ),
        (
            CorePrimitiveIntrinsic::VmNativeBridgeStart,
            "vm.native_bridge.start",
        ),
        (
            CorePrimitiveIntrinsic::VmNativeBridgeCall,
            "vm.native_bridge.call",
        ),
        (
            CorePrimitiveIntrinsic::VmNativeBridgeDispose,
            "vm.native_bridge.dispose",
        ),
        (
            CorePrimitiveIntrinsic::VmNativeBridgeStop,
            "vm.native_bridge.stop",
        ),
        (
            CorePrimitiveIntrinsic::VmBytesFromList,
            "vm.bytes.from_list",
        ),
        (CorePrimitiveIntrinsic::VmBytesToList, "vm.bytes.to_list"),
        (CorePrimitiveIntrinsic::VmBytesLength, "vm.bytes.length"),
        (CorePrimitiveIntrinsic::VmBytesConcat, "vm.bytes.concat"),
        (CorePrimitiveIntrinsic::VmBytesSlice, "vm.bytes.slice"),
        (
            CorePrimitiveIntrinsic::VmBytesReadUintBe,
            "vm.bytes.read_uint_be",
        ),
        (
            CorePrimitiveIntrinsic::VmBytesReadIntBe,
            "vm.bytes.read_int_be",
        ),
        (
            CorePrimitiveIntrinsic::VmBytesReadUintLe,
            "vm.bytes.read_uint_le",
        ),
        (
            CorePrimitiveIntrinsic::VmBytesReadIntLe,
            "vm.bytes.read_int_le",
        ),
        (
            CorePrimitiveIntrinsic::VmTimeoutMilliseconds,
            "vm.timeout.milliseconds",
        ),
        (
            CorePrimitiveIntrinsic::VmTimeoutForever,
            "vm.timeout.forever",
        ),
        (CorePrimitiveIntrinsic::VmTcpListen, "vm.tcp.listen"),
        (
            CorePrimitiveIntrinsic::VmTcpListenWithBacklog,
            "vm.tcp.listen_with_backlog",
        ),
        (CorePrimitiveIntrinsic::VmTcpAccept, "vm.tcp.accept"),
        (CorePrimitiveIntrinsic::VmTcpConnect, "vm.tcp.connect"),
        (CorePrimitiveIntrinsic::VmTcpSend, "vm.tcp.send"),
        (CorePrimitiveIntrinsic::VmTcpReceive, "vm.tcp.receive"),
        (CorePrimitiveIntrinsic::VmTcpClose, "vm.tcp.close"),
        (
            CorePrimitiveIntrinsic::VmTcpCloseListener,
            "vm.tcp.close_listener",
        ),
        (CorePrimitiveIntrinsic::VmPortOpen, "vm.port.open"),
        (CorePrimitiveIntrinsic::VmPortWrite, "vm.port.write"),
        (CorePrimitiveIntrinsic::VmPortRead, "vm.port.read"),
        (CorePrimitiveIntrinsic::VmPortClose, "vm.port.close"),
        (
            CorePrimitiveIntrinsic::VmSupervisorStartRoot,
            "vm.supervisor.start_root",
        ),
        (
            CorePrimitiveIntrinsic::VmSupervisorChildSpec,
            "vm.supervisor.child_spec",
        ),
        (
            CorePrimitiveIntrinsic::VmSupervisorStart,
            "vm.supervisor.start",
        ),
        (
            CorePrimitiveIntrinsic::VmSupervisorStop,
            "vm.supervisor.stop",
        ),
        (CorePrimitiveIntrinsic::VmTaskStart, "vm.task.start"),
        (CorePrimitiveIntrinsic::VmTaskResult, "vm.task.result"),
        (CorePrimitiveIntrinsic::VmTaskCancel, "vm.task.cancel"),
    ] {
        assert_eq!(intrinsic.registry_key(), key);
        assert!(
            !intrinsic.registry_key().starts_with("beam."),
            "std.vm registry keys must not use BEAM vocabulary"
        );
    }
}

/// Verifies VM primitive return-type metadata is available to CoreIR.
///
/// Inputs:
/// - Representative byte, timeout, TCP, and port intrinsic variants.
///
/// Output:
/// - Test passes when the return-type table exposes the source-level result
///   shapes used by type preservation and backend lowering.
///
/// Transformation:
/// - Exercises the new return-type rows without involving source parsing or a
///   specific target backend.
#[test]
fn vm_library_primitive_return_types_are_registered() {
    assert_eq!(
        core_primitive_intrinsic_return_type(&CorePrimitiveIntrinsic::VmBytesLength),
        CoreType::Int
    );
    assert_eq!(
        core_primitive_intrinsic_return_type(&CorePrimitiveIntrinsic::VmBytesSlice),
        CoreType::Named("Bytes".to_string())
    );
    assert_eq!(
        core_primitive_intrinsic_return_type(&CorePrimitiveIntrinsic::VmBytesReadUintBe),
        CoreType::Int
    );
    assert_eq!(
        core_primitive_intrinsic_return_type(&CorePrimitiveIntrinsic::VmBytesReadIntBe),
        CoreType::Int
    );
    assert_eq!(
        core_primitive_intrinsic_return_type(&CorePrimitiveIntrinsic::VmBytesReadUintLe),
        CoreType::Int
    );
    assert_eq!(
        core_primitive_intrinsic_return_type(&CorePrimitiveIntrinsic::VmBytesReadIntLe),
        CoreType::Int
    );
    assert_eq!(
        core_primitive_intrinsic_return_type(&CorePrimitiveIntrinsic::VmTimeoutForever),
        CoreType::Named("Timeout".to_string())
    );
    assert_eq!(
        core_primitive_intrinsic_return_type(&CorePrimitiveIntrinsic::VmTcpListen),
        CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("TcpListener".to_string()),
                CoreType::Named("Error".to_string())
            ],
        }
    );
    assert_eq!(
        core_primitive_intrinsic_return_type(&CorePrimitiveIntrinsic::VmTcpAccept),
        CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("TcpSocket".to_string()),
                CoreType::Named("Error".to_string())
            ],
        }
    );
    assert_eq!(
        core_primitive_intrinsic_return_type(&CorePrimitiveIntrinsic::VmTcpConnect),
        CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("TcpSocket".to_string()),
                CoreType::Named("Error".to_string())
            ],
        }
    );
    assert_eq!(
        core_primitive_intrinsic_return_type(&CorePrimitiveIntrinsic::VmPortRead),
        CoreType::Apply {
            constructor: "Result".to_string(),
            args: vec![
                CoreType::Named("Bytes".to_string()),
                CoreType::Named("Error".to_string())
            ],
        }
    );
}

/// Verifies unsupported VM primitive arities are rejected by CoreIR selection.
///
/// Inputs:
/// - Known VM primitive names with deliberately wrong arities.
///
/// Output:
/// - Test passes when the selector returns `None`.
///
/// Transformation:
/// - Ensures capability admission remains exact so malformed source or stale
///   summaries cannot be silently lowered as target runtime operations.
#[test]
fn core_primitive_intrinsic_rejects_wrong_vm_primitive_arities() {
    assert_eq!(
        core_primitive_intrinsic("std.vm.Bytes", "from_list", 0),
        None
    );
    assert_eq!(core_primitive_intrinsic("std.vm.Bytes", "slice", 2), None);
    assert_eq!(
        core_primitive_intrinsic("std.vm.Bytes", "read_uint_be", 2),
        None
    );
    assert_eq!(
        core_primitive_intrinsic("std.vm.Bytes", "read_int_be", 2),
        None
    );
    assert_eq!(
        core_primitive_intrinsic("std.vm.Bytes", "read_uint_le", 2),
        None
    );
    assert_eq!(
        core_primitive_intrinsic("std.vm.Bytes", "read_int_le", 2),
        None
    );
    assert_eq!(
        core_primitive_intrinsic("std.vm.Timeout", "forever", 1),
        None
    );
    assert_eq!(core_primitive_intrinsic("std.vm.Tcp", "connect", 2), None);
    assert_eq!(core_primitive_intrinsic("std.vm.Tcp", "listen", 2), None);
    assert_eq!(core_primitive_intrinsic("std.vm.Tcp", "accept", 1), None);
    assert_eq!(core_primitive_intrinsic("std.vm.Port", "read", 2), None);
}

#[test]
fn vm_bitstring_intrinsics_have_closed_ids_and_return_types() {
    for (function, arity, intrinsic, return_type) in [
        (
            "from_bytes",
            2,
            CorePrimitiveIntrinsic::VmBitStringFromBytes,
            CoreType::Named("BitString".to_string()),
        ),
        (
            "from_uint_be",
            2,
            CorePrimitiveIntrinsic::VmBitStringFromUintBe,
            CoreType::Named("BitString".to_string()),
        ),
        (
            "from_int_be",
            2,
            CorePrimitiveIntrinsic::VmBitStringFromIntBe,
            CoreType::Named("BitString".to_string()),
        ),
        (
            "from_uint_le",
            2,
            CorePrimitiveIntrinsic::VmBitStringFromUintLe,
            CoreType::Named("BitString".to_string()),
        ),
        (
            "from_int_le",
            2,
            CorePrimitiveIntrinsic::VmBitStringFromIntLe,
            CoreType::Named("BitString".to_string()),
        ),
        (
            "utf8_scalar",
            1,
            CorePrimitiveIntrinsic::VmBitStringUtf8Scalar,
            CoreType::Named("BitString".to_string()),
        ),
        (
            "to_utf8_scalar",
            1,
            CorePrimitiveIntrinsic::VmBitStringToUtf8Scalar,
            CoreType::Int,
        ),
        (
            "utf16_be_scalar",
            1,
            CorePrimitiveIntrinsic::VmBitStringUtf16BeScalar,
            CoreType::Named("BitString".to_string()),
        ),
        (
            "utf16_le_scalar",
            1,
            CorePrimitiveIntrinsic::VmBitStringUtf16LeScalar,
            CoreType::Named("BitString".to_string()),
        ),
        (
            "to_utf16_be_scalar",
            1,
            CorePrimitiveIntrinsic::VmBitStringToUtf16BeScalar,
            CoreType::Int,
        ),
        (
            "to_utf16_le_scalar",
            1,
            CorePrimitiveIntrinsic::VmBitStringToUtf16LeScalar,
            CoreType::Int,
        ),
        (
            "utf32_be_scalar",
            1,
            CorePrimitiveIntrinsic::VmBitStringUtf32BeScalar,
            CoreType::Named("BitString".to_string()),
        ),
        (
            "utf32_le_scalar",
            1,
            CorePrimitiveIntrinsic::VmBitStringUtf32LeScalar,
            CoreType::Named("BitString".to_string()),
        ),
        (
            "to_utf32_be_scalar",
            1,
            CorePrimitiveIntrinsic::VmBitStringToUtf32BeScalar,
            CoreType::Int,
        ),
        (
            "to_utf32_le_scalar",
            1,
            CorePrimitiveIntrinsic::VmBitStringToUtf32LeScalar,
            CoreType::Int,
        ),
        (
            "bit_length",
            1,
            CorePrimitiveIntrinsic::VmBitStringBitLength,
            CoreType::Int,
        ),
        (
            "byte_length",
            1,
            CorePrimitiveIntrinsic::VmBitStringByteLength,
            CoreType::Int,
        ),
        (
            "is_byte_aligned",
            1,
            CorePrimitiveIntrinsic::VmBitStringIsByteAligned,
            CoreType::Bool,
        ),
        (
            "slice",
            3,
            CorePrimitiveIntrinsic::VmBitStringSlice,
            CoreType::Named("BitString".to_string()),
        ),
        (
            "concat",
            2,
            CorePrimitiveIntrinsic::VmBitStringConcat,
            CoreType::Named("BitString".to_string()),
        ),
        (
            "to_bytes",
            1,
            CorePrimitiveIntrinsic::VmBitStringToBytes,
            CoreType::Named("Bytes".to_string()),
        ),
        (
            "to_uint_be",
            1,
            CorePrimitiveIntrinsic::VmBitStringToUintBe,
            CoreType::Int,
        ),
        (
            "to_int_be",
            1,
            CorePrimitiveIntrinsic::VmBitStringToIntBe,
            CoreType::Int,
        ),
        (
            "to_uint_le",
            1,
            CorePrimitiveIntrinsic::VmBitStringToUintLe,
            CoreType::Int,
        ),
        (
            "to_int_le",
            1,
            CorePrimitiveIntrinsic::VmBitStringToIntLe,
            CoreType::Int,
        ),
    ] {
        assert_eq!(
            core_primitive_intrinsic("std.vm.BitString", function, arity),
            Some(intrinsic.clone())
        );
        assert!(intrinsic.registry_key().starts_with("vm.bitstring."));
        assert_eq!(
            core_primitive_intrinsic_return_type(&intrinsic),
            return_type
        );
        assert_eq!(
            core_primitive_intrinsic("std.vm.BitString", function, arity + 1),
            None
        );
    }
}
