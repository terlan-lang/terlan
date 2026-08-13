use std::collections::{HashMap, HashSet};

use super::*;
use crate::compiler::native_ir::call_composition::{
    ComposedCallProfile, DynamicCallSignature, DynamicTargetProfile,
};
use crate::compiler::native_ir::NativeType;

fn target(export_id: u64, source: &str) -> DynamicTargetProfile {
    DynamicTargetProfile {
        export_id,
        source: source.to_string(),
        profile: ComposedCallProfile::pure(),
    }
}

#[test]
fn restriction_preserves_known_pure_callback_targets() {
    let signature = DynamicCallSignature {
        parameters: vec![NativeType::Int],
        result: NativeType::Bool,
    };
    let profiles = HashMap::from([(
        signature.clone(),
        vec![target(11, "pure"), target(22, "suspending")],
    )]);

    let restricted = restrict_profiles(&profiles, Some(&HashSet::from([11])));

    assert_eq!(restricted[&signature].len(), 1);
    assert_eq!(restricted[&signature][0].export_id, 11);
    assert!(restricted[&signature][0].profile.continuations.is_empty());
    validate_profiles(
        &restricted,
        Some(&HashSet::from([11])),
        "test owner",
        &HashMap::new(),
    )
    .expect("pure target remains an explicit closed-world target");
}

#[test]
fn validation_rejects_an_unprofiled_closed_world_target() {
    let signature = DynamicCallSignature {
        parameters: vec![NativeType::Int],
        result: NativeType::Bool,
    };
    let profiles = HashMap::from([(signature, vec![target(11, "known")])]);

    let error = validate_profiles(
        &profiles,
        Some(&HashSet::from([22])),
        "test owner",
        &HashMap::from([(22, "missing continuation".to_string())]),
    )
    .expect_err("missing target must fail closed");

    let error = error.to_string();
    assert!(error.contains("closure targets [22]"));
    assert!(error.contains("missing continuation"));
    assert!(error.contains("(11, \"known\")"));
}
