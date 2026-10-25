//! Bounded decoder for recursive pattern metadata in the self-host ABI.

use crate::self_host_ir::IrInstruction;

const MAX_PATTERN_NODES: usize = 4_096;
const MAX_PATTERN_DEPTH: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PatternNode {
    pub(super) kind: String,
    pub(super) text: String,
    pub(super) children: Vec<PatternNode>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PatternBinding {
    pub(super) name: String,
    pub(super) path: Vec<usize>,
}

impl PatternNode {
    pub(super) fn irrefutable(&self) -> bool {
        matches!(
            self.kind.as_str(),
            "binding-pattern" | "wildcard-pattern" | "pattern-token"
        )
    }

    pub(super) fn bindings(&self) -> Vec<PatternBinding> {
        let mut bindings = Vec::new();
        self.collect_bindings(&mut Vec::new(), &mut bindings);
        bindings
    }

    fn collect_bindings(&self, path: &mut Vec<usize>, output: &mut Vec<PatternBinding>) {
        if self.kind == "binding-pattern" && self.text != "_" {
            output.push(PatternBinding {
                name: self.text.clone(),
                path: path.clone(),
            });
        }
        if self.kind == "string-capture-segment" {
            let name = self
                .text
                .split_once(':')
                .map_or(self.text.as_str(), |(name, _)| name)
                .trim();
            if !name.is_empty() && name != "_" {
                output.push(PatternBinding {
                    name: name.to_string(),
                    path: path.clone(),
                });
            }
        }
        for (index, child) in self.children.iter().enumerate() {
            path.push(index);
            child.collect_bindings(path, output);
            path.pop();
        }
    }
}

/// Decodes the subject SSA identity and one complete recursive pattern tree.
pub(super) fn decode_prepare_pattern(
    instruction: &IrInstruction,
) -> Result<(&str, PatternNode), String> {
    if instruction.opcode != "prepare-pattern" {
        return Err("error[self_host.native.pattern_opcode]: expected prepare-pattern".into());
    }
    let (subject, encoded) = instruction.operands.split_first().ok_or_else(|| {
        "error[self_host.native.pattern_encoding]: pattern has no subject operand".to_string()
    })?;
    let deferred = encoded.first().is_some_and(|value| value == "pattern-v2");
    let mut cursor = PatternCursor {
        encoded,
        offset: usize::from(deferred),
        nodes: 0,
    };
    let pattern = cursor.node(0)?;
    if cursor.offset != encoded.len() {
        return Err(format!(
            "error[self_host.native.pattern_encoding]: pattern has {} trailing operand(s)",
            encoded.len() - cursor.offset
        ));
    }
    Ok((subject, pattern))
}

pub(super) fn prepare_pattern_deferred(instruction: &IrInstruction) -> bool {
    instruction
        .operands
        .get(1)
        .is_some_and(|value| value == "pattern-v2")
}

/// Decodes a v2 binding selector, source type, and recursive pattern tree.
pub(super) fn decode_binding_pattern(
    instruction: &IrInstruction,
) -> Result<(usize, &str, PatternNode), String> {
    if instruction.opcode != "get-pattern-binding" {
        return Err("error[self_host.native.pattern_opcode]: expected get-pattern-binding".into());
    }
    let selector = instruction
        .operands
        .get(1)
        .ok_or_else(|| {
            "error[self_host.native.pattern_binding]: missing binding selector".to_string()
        })?
        .parse::<usize>()
        .map_err(|_| {
            "error[self_host.native.pattern_binding]: binding selector is not an index".to_string()
        })?;
    let source_type = instruction.operands.get(2).ok_or_else(|| {
        "error[self_host.native.pattern_binding]: missing binding source type".to_string()
    })?;
    let encoded = instruction.operands.get(3..).ok_or_else(|| {
        "error[self_host.native.pattern_binding]: missing binding pattern".to_string()
    })?;
    let mut cursor = PatternCursor {
        encoded,
        offset: 0,
        nodes: 0,
    };
    let pattern = cursor.node(0)?;
    if cursor.offset != encoded.len() {
        return Err(format!(
            "error[self_host.native.pattern_encoding]: binding pattern has {} trailing operand(s)",
            encoded.len() - cursor.offset
        ));
    }
    Ok((selector, source_type, pattern))
}

struct PatternCursor<'a> {
    encoded: &'a [String],
    offset: usize,
    nodes: usize,
}

impl PatternCursor<'_> {
    fn node(&mut self, depth: usize) -> Result<PatternNode, String> {
        if depth >= MAX_PATTERN_DEPTH {
            return Err("error[self_host.native.pattern_depth]: pattern exceeds 64 levels".into());
        }
        self.nodes = self.nodes.checked_add(1).ok_or_else(|| {
            "error[self_host.native.pattern_size]: pattern node count overflow".to_string()
        })?;
        if self.nodes > MAX_PATTERN_NODES {
            return Err("error[self_host.native.pattern_size]: pattern exceeds 4096 nodes".into());
        }
        let kind = self.next("kind")?.to_owned();
        let text = self.next("text")?.to_owned();
        let count_text = self.next("child count")?;
        let child_count = count_text.parse::<usize>().map_err(|_| {
            format!("error[self_host.native.pattern_encoding]: invalid child count {count_text:?}")
        })?;
        if child_count > MAX_PATTERN_NODES.saturating_sub(self.nodes) {
            return Err(
                "error[self_host.native.pattern_size]: declared children exceed node bound".into(),
            );
        }
        let mut children = Vec::with_capacity(child_count);
        for _ in 0..child_count {
            children.push(self.node(depth + 1)?);
        }
        Ok(PatternNode {
            kind,
            text,
            children,
        })
    }

    fn next(&mut self, label: &str) -> Result<&str, String> {
        let value = self.encoded.get(self.offset).ok_or_else(|| {
            format!("error[self_host.native.pattern_encoding]: missing pattern {label}")
        })?;
        self.offset += 1;
        Ok(value)
    }
}

#[cfg(test)]
#[path = "self_host_pattern_ir_inline_test.rs"]
mod tests;
