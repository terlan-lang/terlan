//! Closed member types for native-package calls accepting transparent record unions.
use super::{BoundaryError, ErrorDomain, MAX_FRAME_BYTES};
use crate::runtime::native_image::TvmBoundaryType;

pub(crate) const TAG: i64 = 60;
const PREFIX: usize = 7;

pub(crate) struct Argument<'a> {
    pub(crate) types: &'a [i64],
    pub(crate) word: i64,
}
/// Separates a bounded variable frame from its continuation captures.
pub(crate) fn frame_words(words: &[i64]) -> Result<usize, BoundaryError> {
    let length = words
        .get(6)
        .and_then(|value| usize::try_from(*value).ok())
        .ok_or_else(invalid)?;
    if words.first() != Some(&TAG)
        || !(PREFIX..=MAX_FRAME_BYTES / 8).contains(&length)
        || length > words.len()
    {
        return Err(invalid());
    }
    arguments(&words[..length])?;
    Ok(length)
}
/// Checks every alternative before owner-checked decoding.
pub(crate) fn arguments(words: &[i64]) -> Result<Vec<Argument<'_>>, BoundaryError> {
    if words.len() < PREFIX
        || words[0] != TAG
        || usize::try_from(words[6]).ok() != Some(words.len())
    {
        return Err(invalid());
    }
    let count = usize::try_from(words[5]).map_err(|_| invalid())?;
    if count > words.len() / 5 {
        return Err(invalid());
    }
    let mut result = Vec::with_capacity(count);
    let mut offset = PREFIX;
    for _ in 0..count {
        let alternatives = words
            .get(offset)
            .and_then(|value| usize::try_from(*value).ok())
            .filter(|count| (1..=16).contains(count))
            .ok_or_else(invalid)?;
        offset += 1;
        let end = offset
            .checked_add(alternatives * 3)
            .filter(|end| *end < words.len())
            .ok_or_else(invalid)?;
        let types = &words[offset..end];
        for metadata in types.chunks_exact(3) {
            let ty = TvmBoundaryType::from_transition_words(metadata).map_err(|_| invalid())?;
            if alternatives > 1 && !matches!(ty, TvmBoundaryType::Managed(_)) {
                return Err(invalid());
            }
        }
        result.push(Argument {
            types,
            word: words[end],
        });
        offset = end + 1;
    }
    if offset != words.len() {
        return Err(invalid());
    }
    Ok(result)
}
fn invalid() -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::NativeImageAdmission,
        "native package union frame",
        "error[pure_native_capability_arguments]: invalid bounded package union frame",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn union_frame_is_bounded_and_excludes_continuation_captures() {
        let mut frame = vec![TAG, 0, 0, 0, 99, 1, 15, 2];
        frame.extend(TvmBoundaryType::Managed([1; 16]).transition_words());
        frame.extend(TvmBoundaryType::Managed([2; 16]).transition_words());
        frame.push(42);
        frame.push(999);
        assert_eq!(frame_words(&frame).unwrap(), 15);
        assert_eq!(arguments(&frame[..15]).unwrap()[0].word, 42);
        for bad in [-1, 0, 17, i64::MAX] {
            let mut changed = frame.clone();
            changed[7] = bad;
            assert!(frame_words(&changed).is_err());
        }
        frame[6] = 16;
        assert!(frame_words(&frame).is_err());
    }
    #[test]
    fn union_alternatives_cannot_reinterpret_scalar_words() {
        let mut frame = vec![TAG, 0, 0, 0, 99, 1, 15, 2];
        frame.extend(TvmBoundaryType::Int.transition_words());
        frame.extend(TvmBoundaryType::Managed([2; 16]).transition_words());
        frame.push(42);
        assert!(arguments(&frame).is_err());
    }
}
