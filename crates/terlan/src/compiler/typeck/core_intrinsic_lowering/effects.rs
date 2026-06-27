use super::CoreEffectSet;

/// Builds the canonical pure Core effect set.
///
/// Inputs:
/// - None.
///
/// Output:
/// - `CoreEffectSet` containing the stable `pure` label.
///
/// Transformation:
/// - Centralizes the effect payload used by primitive intrinsics that do not
///   perform observable side effects.
pub(crate) fn core_pure_effect_set() -> CoreEffectSet {
    CoreEffectSet {
        effects: vec!["pure".to_string()],
    }
}

/// Builds the canonical IO Core effect set.
///
/// Inputs:
/// - None.
///
/// Output:
/// - `CoreEffectSet` containing the stable `io` label.
///
/// Transformation:
/// - Centralizes the effect payload used by runtime capabilities that perform
///   observable console or stream effects.
pub(crate) fn core_io_effect_set() -> CoreEffectSet {
    CoreEffectSet {
        effects: vec!["io".to_string()],
    }
}

/// Builds the canonical mutable receiver Core effect set.
///
/// Inputs:
/// - None.
///
/// Output:
/// - `CoreEffectSet` containing the stable `receiver_mutation` label.
///
/// Transformation:
/// - Centralizes the effect payload used by receiver methods whose source
///   receiver is declared mutable, keeping mutation separate from ordinary
///   `Unit`-returning calls in CoreIR.
pub(crate) fn core_receiver_mutation_effect_set() -> CoreEffectSet {
    CoreEffectSet {
        effects: vec!["receiver_mutation".to_string()],
    }
}
