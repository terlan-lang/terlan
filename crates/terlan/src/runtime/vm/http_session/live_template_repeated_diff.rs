use std::collections::BTreeSet;

use super::{
    validate_live_template_patch_payload, ReplValue, VmHttpSession,
    VmHttpSessionLiveTemplateSourceSpan, VmHttpSessionLiveTemplateStateFanout,
    VmHttpSessionRuntime,
};

/// One stable-keyed value rendered inside a repeated live-template fragment.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmHttpSessionLiveTemplateRepeatedBinding {
    pub(crate) key: String,
    pub(crate) value: ReplValue,
}

impl VmHttpSessionLiveTemplateRepeatedBinding {
    pub(crate) fn new(key: impl Into<String>, value: ReplValue) -> Self {
        Self {
            key: key.into(),
            value,
        }
    }
}

/// One rendered stable-keyed fragment after typed interpolation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmHttpSessionLiveTemplateRenderedFragment {
    pub(crate) key: String,
    pub(crate) content: String,
}

/// Deterministic operation applied to a repeated fragment list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmHttpSessionLiveTemplateRepeatedPatch {
    Insert {
        index: usize,
        key: String,
        content: String,
    },
    Remove {
        index: usize,
        key: String,
    },
    Move {
        from: usize,
        to: usize,
        key: String,
    },
    Replace {
        index: usize,
        key: String,
        content: String,
    },
}

/// Stable patch plan and final rendered fragment order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmHttpSessionLiveTemplateRepeatedDiff {
    pub(crate) operations: Vec<VmHttpSessionLiveTemplateRepeatedPatch>,
    pub(crate) rendered_fragments: Vec<VmHttpSessionLiveTemplateRenderedFragment>,
}

/// Actor fanout paired with the repeated-fragment diff that produced it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmHttpSessionLiveTemplateRepeatedFanout {
    pub(crate) diff: VmHttpSessionLiveTemplateRepeatedDiff,
    pub(crate) fanout: VmHttpSessionLiveTemplateStateFanout,
}

impl VmHttpSessionRuntime {
    /// Renders and fans out one deterministic repeated-fragment state transition.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn fanout_live_template_repeated_state_update(
        &mut self,
        session: &VmHttpSession,
        expected_version: u64,
        patch_event: &str,
        source: &VmHttpSessionLiveTemplateSourceSpan,
        previous: &[VmHttpSessionLiveTemplateRepeatedBinding],
        current: &[VmHttpSessionLiveTemplateRepeatedBinding],
        render: impl FnMut(&ReplValue) -> Result<String, String>,
        update: impl FnOnce(&mut Self, &VmHttpSession) -> Result<(), String>,
    ) -> Result<VmHttpSessionLiveTemplateRepeatedFanout, String> {
        let diff = build_live_template_repeated_diff(previous, current, source, render)?;
        let patch_payload = repeated_diff_payload(&diff)?;
        let fanout = self.fanout_live_template_state_update(
            session,
            expected_version,
            patch_event,
            source,
            patch_payload,
            update,
        )?;
        Ok(VmHttpSessionLiveTemplateRepeatedFanout { diff, fanout })
    }
}

pub(crate) fn build_live_template_repeated_diff(
    previous: &[VmHttpSessionLiveTemplateRepeatedBinding],
    current: &[VmHttpSessionLiveTemplateRepeatedBinding],
    source: &VmHttpSessionLiveTemplateSourceSpan,
    mut render: impl FnMut(&ReplValue) -> Result<String, String>,
) -> Result<VmHttpSessionLiveTemplateRepeatedDiff, String> {
    validate_repeated_bindings("previous", previous, source)?;
    validate_repeated_bindings("current", current, source)?;
    let mut working = render_repeated_bindings(previous, source, &mut render)?;
    let rendered_fragments = render_repeated_bindings(current, source, &mut render)?;
    let mut operations = Vec::new();

    for (target_index, target) in rendered_fragments.iter().enumerate() {
        match working
            .iter()
            .position(|fragment| fragment.key == target.key)
        {
            None => {
                working.insert(target_index, target.clone());
                operations.push(VmHttpSessionLiveTemplateRepeatedPatch::Insert {
                    index: target_index,
                    key: target.key.clone(),
                    content: target.content.clone(),
                });
            }
            Some(source_index) => {
                if source_index != target_index {
                    let moved = working.remove(source_index);
                    working.insert(target_index, moved);
                    operations.push(VmHttpSessionLiveTemplateRepeatedPatch::Move {
                        from: source_index,
                        to: target_index,
                        key: target.key.clone(),
                    });
                }
                if working[target_index].content != target.content {
                    working[target_index].content = target.content.clone();
                    operations.push(VmHttpSessionLiveTemplateRepeatedPatch::Replace {
                        index: target_index,
                        key: target.key.clone(),
                        content: target.content.clone(),
                    });
                }
            }
        }
    }

    while working.len() > rendered_fragments.len() {
        let index = working.len() - 1;
        let removed = working.remove(index);
        operations.push(VmHttpSessionLiveTemplateRepeatedPatch::Remove {
            index,
            key: removed.key,
        });
    }

    debug_assert_eq!(working, rendered_fragments);
    Ok(VmHttpSessionLiveTemplateRepeatedDiff {
        operations,
        rendered_fragments,
    })
}

fn validate_repeated_bindings(
    side: &str,
    bindings: &[VmHttpSessionLiveTemplateRepeatedBinding],
    source: &VmHttpSessionLiveTemplateSourceSpan,
) -> Result<(), String> {
    let mut keys = BTreeSet::new();
    for binding in bindings {
        if binding.key.is_empty()
            || binding.key.trim() != binding.key
            || binding.key.chars().any(char::is_control)
        {
            return Err(repeated_diff_error(
                source,
                format!("{side} fragment key must be non-empty, normalized, and control-free"),
            ));
        }
        if !keys.insert(binding.key.as_str()) {
            return Err(repeated_diff_error(
                source,
                format!("{side} fragment key `{}` is duplicated", binding.key),
            ));
        }
        validate_live_template_patch_payload(&binding.value, source)?;
    }
    Ok(())
}

fn render_repeated_bindings(
    bindings: &[VmHttpSessionLiveTemplateRepeatedBinding],
    source: &VmHttpSessionLiveTemplateSourceSpan,
    render: &mut impl FnMut(&ReplValue) -> Result<String, String>,
) -> Result<Vec<VmHttpSessionLiveTemplateRenderedFragment>, String> {
    bindings
        .iter()
        .map(|binding| {
            let content = render(&binding.value).map_err(|detail| {
                repeated_diff_error(
                    source,
                    format!("fragment `{}` render failed: {detail}", binding.key),
                )
            })?;
            Ok(VmHttpSessionLiveTemplateRenderedFragment {
                key: binding.key.clone(),
                content,
            })
        })
        .collect()
}

fn repeated_diff_payload(
    diff: &VmHttpSessionLiveTemplateRepeatedDiff,
) -> Result<ReplValue, String> {
    let operations = diff
        .operations
        .iter()
        .map(repeated_patch_value)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ReplValue::Tuple(vec![
        ReplValue::Atom("repeated_fragment_diff".to_string()),
        ReplValue::List(operations),
    ]))
}

fn repeated_patch_value(
    patch: &VmHttpSessionLiveTemplateRepeatedPatch,
) -> Result<ReplValue, String> {
    let value = match patch {
        VmHttpSessionLiveTemplateRepeatedPatch::Insert {
            index,
            key,
            content,
        } => ReplValue::Tuple(vec![
            ReplValue::Atom("insert".to_string()),
            ReplValue::Int(index_to_int(*index)?),
            ReplValue::String(key.clone()),
            ReplValue::String(content.clone()),
        ]),
        VmHttpSessionLiveTemplateRepeatedPatch::Remove { index, key } => ReplValue::Tuple(vec![
            ReplValue::Atom("remove".to_string()),
            ReplValue::Int(index_to_int(*index)?),
            ReplValue::String(key.clone()),
        ]),
        VmHttpSessionLiveTemplateRepeatedPatch::Move { from, to, key } => ReplValue::Tuple(vec![
            ReplValue::Atom("move".to_string()),
            ReplValue::Int(index_to_int(*from)?),
            ReplValue::Int(index_to_int(*to)?),
            ReplValue::String(key.clone()),
        ]),
        VmHttpSessionLiveTemplateRepeatedPatch::Replace {
            index,
            key,
            content,
        } => ReplValue::Tuple(vec![
            ReplValue::Atom("replace".to_string()),
            ReplValue::Int(index_to_int(*index)?),
            ReplValue::String(key.clone()),
            ReplValue::String(content.clone()),
        ]),
    };
    Ok(value)
}

fn index_to_int(index: usize) -> Result<i64, String> {
    i64::try_from(index)
        .map_err(|_| "HTTP live-template repeated fragment index overflowed Int".to_string())
}

fn repeated_diff_error(
    source: &VmHttpSessionLiveTemplateSourceSpan,
    detail: impl std::fmt::Display,
) -> String {
    format!(
        "template_runtime_actor_bind_error: {}:{}:{}: HTTP live-template repeated fragment {detail}",
        source.module, source.line, source.column
    )
}
