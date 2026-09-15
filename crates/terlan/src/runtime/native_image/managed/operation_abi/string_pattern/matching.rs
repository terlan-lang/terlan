//! Linear delimiter matching with anchored prefix/suffix and checked captures.

use super::{Kind, Segment};

#[derive(Debug, PartialEq)]
pub(super) enum Captured<'a> {
    String(&'a str),
    Int(i64),
    Float(f64),
    Bool(bool),
}

pub(super) fn capture<'a>(text: &'a str, segments: &[Segment<'_>]) -> Option<Vec<Captured<'a>>> {
    let mut cursor = 0;
    let mut captures = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        let rest = text.get(cursor..)?;
        match segment {
            Segment::Literal(literal) => {
                rest.strip_prefix(literal)?;
                cursor += literal.len();
            }
            Segment::Capture(kind) => {
                let length = match segments.get(index + 1) {
                    None => rest.len(),
                    Some(Segment::Literal(literal)) if index + 2 == segments.len() => {
                        rest.strip_suffix(literal)?.len()
                    }
                    Some(Segment::Literal(literal)) => rest.find(literal)?,
                    Some(Segment::Capture(_)) => return None,
                };
                let value = rest.get(..length)?;
                captures.push(match kind {
                    Kind::String => Captured::String(value),
                    Kind::Int => {
                        Captured::Int(super::super::integer::parse_integer_text(value, 10)?)
                    }
                    Kind::Float => Captured::Float(super::super::float::parse_float_text(value)?),
                    Kind::Bool => Captured::Bool(value.parse().ok()?),
                });
                cursor += length;
            }
        }
    }
    (cursor == text.len()).then_some(captures)
}
