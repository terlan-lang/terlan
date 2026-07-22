use super::*;
use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;

#[test]
fn syntax_output_lowering_canonicalizes_process_receive_int_transition() {
    let module = parse_module_as_syntax_output(
        "module core_process_receive.\n\nimport std.vm.Process.\n\npub demo(): Int ->\n    Process.receive_int().\n",
    )
    .expect("parse imported Process Receive fixture");
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);
    let body = core
        .functions
        .iter()
        .find(|function| function.name == "demo")
        .and_then(|function| function.clauses.first())
        .and_then(|clause| clause.body.core_expr.as_ref())
        .expect("demo CoreIR body");
    assert!(matches!(
        body,
        CoreExpr::Intrinsic(CoreIntrinsicCall {
            id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmProcessReceiveInt),
            args,
            return_type: CoreType::Int,
            effects,
            ..
        }) if args.is_empty()
            && effects.effects.iter().any(|effect| effect == "vm_effect_execution")
    ));
}

#[test]
fn syntax_output_lowering_canonicalizes_typed_mailbox_operations() {
    let module = parse_module_as_syntax_output(
        concat!(
            "module core_process_typed.\n\nimport std.vm.Process.\nimport std.vm.Message.\nimport type std.vm.Process.{Process}.\nimport type std.vm.Message.{Message}.\n\n",
            "pub struct Pair {left: Int, right: Int}.\n",
            "pub send_string(recipient: Int, payload: String): Unit -> Process.send_string(recipient, payload).\n",
            "pub receive_string(): String -> Process.receive_string().\n",
            "pub send_bytes(recipient: Int, payload: Bytes): Unit -> Process.send_bytes(recipient, payload).\n",
            "pub receive_bytes(): Bytes -> Process.receive_bytes().\n",
            "pub send_binary(recipient: Int, payload: Binary): Unit -> Process.send_binary(recipient, payload).\n",
            "pub receive_binary(): Binary -> Process.receive_binary().\n",
            "pub send_atom(recipient: Int, payload: Atom): Unit -> Process.send_atom(recipient, payload).\n",
            "pub receive_atom(): Atom -> Process.receive_atom().\n",
            "pub send_pair(recipient: Process[Pair], payload: Message[Pair]): Unit -> Process.send[Pair](recipient, payload).\n",
            "pub receive_pair(): Message[Pair] -> Process.receive[Pair]().\n",
            "pub wrap_pair(payload: Pair): Message[Pair] -> Message.wrap[Pair](payload).\n",
            "pub unwrap_pair(message: Message[Pair]): Pair -> Message.unwrap[Pair](message).\n",
        ),
    )
    .expect("parse typed Process fixture");
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);
    let cases = [
        (
            "send_string",
            CorePrimitiveIntrinsic::VmProcessSendString,
            2,
            CoreType::Named("Unit".to_owned()),
        ),
        (
            "receive_string",
            CorePrimitiveIntrinsic::VmProcessReceiveString,
            0,
            CoreType::String,
        ),
        (
            "send_bytes",
            CorePrimitiveIntrinsic::VmProcessSendBytes,
            2,
            CoreType::Named("Unit".to_owned()),
        ),
        (
            "receive_bytes",
            CorePrimitiveIntrinsic::VmProcessReceiveBytes,
            0,
            CoreType::Named("Bytes".to_owned()),
        ),
        (
            "send_binary",
            CorePrimitiveIntrinsic::VmProcessSendBinary,
            2,
            CoreType::Named("Unit".to_owned()),
        ),
        (
            "receive_binary",
            CorePrimitiveIntrinsic::VmProcessReceiveBinary,
            0,
            CoreType::Binary,
        ),
        (
            "send_atom",
            CorePrimitiveIntrinsic::VmProcessSendAtom,
            2,
            CoreType::Named("Unit".to_owned()),
        ),
        (
            "receive_atom",
            CorePrimitiveIntrinsic::VmProcessReceiveAtom,
            0,
            CoreType::Atom,
        ),
    ];
    for (name, expected_id, expected_arity, expected_return) in cases {
        let body = core
            .functions
            .iter()
            .find(|function| function.name == name)
            .and_then(|function| function.clauses.first())
            .and_then(|clause| clause.body.core_expr.as_ref())
            .unwrap_or_else(|| panic!("{name} CoreIR body"));
        let CoreExpr::Intrinsic(call) = body else {
            panic!("{name} did not lower to an intrinsic");
        };
        assert_eq!(call.id, CoreIntrinsicId::Primitive(expected_id), "{name}");
        assert_eq!(call.args.len(), expected_arity, "{name}");
        assert_eq!(call.return_type, expected_return, "{name}");
        assert!(
            call.effects
                .effects
                .iter()
                .any(|effect| effect == "vm_effect_execution"),
            "{name} must remain effectful"
        );
    }
    for (name, expected_id, expected_arity, expected_return) in [
        (
            "send_pair",
            CoreIntrinsicId::VmProcessSendMessage(CoreType::Named("Pair".to_owned())),
            2,
            CoreType::Named("Unit".to_owned()),
        ),
        (
            "receive_pair",
            CoreIntrinsicId::VmProcessReceiveMessage(CoreType::Named("Pair".to_owned())),
            0,
            CoreType::Apply {
                constructor: "Message".to_owned(),
                args: vec![CoreType::Named("Pair".to_owned())],
            },
        ),
    ] {
        let body = core
            .functions
            .iter()
            .find(|function| function.name == name)
            .and_then(|function| function.clauses.first())
            .and_then(|clause| clause.body.core_expr.as_ref())
            .unwrap_or_else(|| panic!("{name} CoreIR body"));
        let CoreExpr::Intrinsic(call) = body else {
            panic!("{name} did not lower to a typed intrinsic");
        };
        assert_eq!(call.id, expected_id, "{name}");
        assert_eq!(call.args.len(), expected_arity, "{name}");
        assert_eq!(call.return_type, expected_return, "{name}");
        assert_eq!(
            call.effects.effects,
            ["vm_effect_execution".to_string()],
            "{name}"
        );
    }
    for name in ["wrap_pair", "unwrap_pair"] {
        let body = core
            .functions
            .iter()
            .find(|function| function.name == name)
            .and_then(|function| function.clauses.first())
            .and_then(|clause| clause.body.core_expr.as_ref())
            .unwrap_or_else(|| panic!("{name} CoreIR body"));
        assert!(matches!(body, CoreExpr::Var(_)), "{name}: {body:?}");
    }
}

#[test]
fn syntax_output_lowering_canonicalizes_typed_process_lifecycle_transitions() {
    let module = parse_module_as_syntax_output(concat!(
        "module core_process_lifecycle.\n\nimport std.vm.Process.\n",
        "import type std.vm.Process.{Entry, ExitReason, Monitor, Process as ProcessHandle, Resource, ResourceKind, SchedulingClass, Timer}.\n\n",
        "pub spawn_child(entry: Entry[Int]): ProcessHandle[Int] -> Process.spawn[Int](entry).\n",
        "pub sleep(timer: Timer): Unit -> Process.sleep(timer).\n",
        "pub link(peer: ProcessHandle[Int]): Unit -> Process.link[Int](peer).\n",
        "pub monitor(peer: ProcessHandle[Int]): Monitor[Int] -> Process.monitor[Int](peer).\n",
        "pub acquire(kind: ResourceKind[Int]): Resource[Int] -> Process.acquire[Int](kind).\n",
        "pub cancel(target: ProcessHandle[Int]): Unit -> Process.cancel[Int](target).\n",
        "pub fail(reason: ExitReason): Unit -> Process.fail(reason).\n",
        "pub schedule(class: SchedulingClass): Unit -> Process.schedule(class).\n",
    ))
    .expect("parse typed Process lifecycle fixture");
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);
    let typed = CoreType::Int;
    let cases = [
        (
            "spawn_child",
            CoreIntrinsicId::VmProcessSpawn(typed.clone()),
            CoreType::Apply {
                constructor: "Process".to_string(),
                args: vec![typed.clone()],
            },
        ),
        (
            "link",
            CoreIntrinsicId::VmProcessLink(typed.clone()),
            CoreType::Named("Unit".to_string()),
        ),
        (
            "monitor",
            CoreIntrinsicId::VmProcessMonitor(typed.clone()),
            CoreType::Apply {
                constructor: "Monitor".to_string(),
                args: vec![typed.clone()],
            },
        ),
        (
            "acquire",
            CoreIntrinsicId::VmProcessAcquireResource(typed.clone()),
            CoreType::Apply {
                constructor: "Resource".to_string(),
                args: vec![typed.clone()],
            },
        ),
        (
            "cancel",
            CoreIntrinsicId::VmProcessCancel(typed),
            CoreType::Named("Unit".to_string()),
        ),
    ];
    for (name, expected_id, expected_return) in cases {
        let body = core
            .functions
            .iter()
            .find(|function| function.name == name)
            .and_then(|function| function.clauses.first())
            .and_then(|clause| clause.body.core_expr.as_ref())
            .unwrap_or_else(|| panic!("{name} CoreIR body"));
        let CoreExpr::Intrinsic(call) = body else {
            panic!("{name} did not lower to an intrinsic: {body:?}");
        };
        assert_eq!(call.id, expected_id, "{name}");
        assert_eq!(call.args.len(), 1, "{name}");
        assert_eq!(call.return_type, expected_return, "{name}");
        assert_eq!(call.effects.effects, ["vm_effect_execution"], "{name}");
    }
    for (name, expected_id) in [
        ("sleep", CorePrimitiveIntrinsic::VmProcessSleep),
        ("fail", CorePrimitiveIntrinsic::VmProcessFail),
        ("schedule", CorePrimitiveIntrinsic::VmProcessSchedule),
    ] {
        let body = core
            .functions
            .iter()
            .find(|function| function.name == name)
            .and_then(|function| function.clauses.first())
            .and_then(|clause| clause.body.core_expr.as_ref())
            .unwrap_or_else(|| panic!("{name} CoreIR body"));
        assert!(matches!(
            body,
            CoreExpr::Intrinsic(CoreIntrinsicCall {
                id: CoreIntrinsicId::Primitive(id),
                args,
                return_type: CoreType::Named(unit),
                effects,
                ..
            }) if id == &expected_id && args.len() == 1 && unit == "Unit"
                && effects.effects == ["vm_effect_execution"]
        ));
    }
}

#[test]
fn syntax_output_lowering_erases_typed_lifecycle_descriptors() {
    let module = parse_module_as_syntax_output(concat!(
        "module core_process_descriptors.\n\nimport std.vm.Process.\n",
        "import type std.vm.Process.{Entry, ExitReason, ResourceKind, SchedulingClass, Timer}.\n\n",
        "pub entry(tag: Int): Entry[Int] -> Process.entry[Int](tag).\n",
        "pub timer(ticks: Int): Timer -> Process.timer(ticks).\n",
        "pub kind(tag: Int): ResourceKind[Int] -> Process.resource_kind[Int](tag).\n",
        "pub reason(code: Int): ExitReason -> Process.exit_reason(code).\n",
        "pub priority(): SchedulingClass -> Process.priority().\n",
        "pub normal(): SchedulingClass -> Process.normal().\n",
        "pub background(): SchedulingClass -> Process.background().\n",
    ))
    .expect("parse Process lifecycle descriptor fixture");
    let resolved = resolve_syntax_module_output(&module).module;
    let core = lower_syntax_module_output_to_core(&module, &resolved);
    for name in ["entry", "timer", "kind", "reason"] {
        let body = core
            .functions
            .iter()
            .find(|function| function.name == name)
            .and_then(|function| function.clauses.first())
            .and_then(|clause| clause.body.core_expr.as_ref())
            .unwrap_or_else(|| panic!("{name} CoreIR body"));
        assert!(matches!(body, CoreExpr::Var(_)), "{name}: {body:?}");
    }
    for (name, expected) in [("priority", 1), ("normal", 2), ("background", 3)] {
        let body = core
            .functions
            .iter()
            .find(|function| function.name == name)
            .and_then(|function| function.clauses.first())
            .and_then(|clause| clause.body.core_expr.as_ref())
            .unwrap_or_else(|| panic!("{name} CoreIR body"));
        assert_eq!(body, &CoreExpr::Int(expected), "{name}");
    }
}
