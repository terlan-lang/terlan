//! Compiler-proven projection of the opaque source-visible HTTP Request value.

/// Fields in the fixed managed Request envelope that one AOT export may observe.
///
/// `Complete` is the fail-closed representation. `Fields` is emitted only when
/// typed NativeIR proves that the request cannot escape and every observation is
/// an exact opaque Request accessor.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) enum RequestFieldProjection {
    Complete,
    Fields(u16),
}

impl RequestFieldProjection {
    pub(crate) const METHOD: usize = 1;
    pub(crate) const PATH: usize = 2;
    pub(crate) const PARAMS: usize = 3;
    pub(crate) const BODY: usize = 4;
    pub(crate) const QUERY_STRING: usize = 5;
    pub(crate) const QUERY: usize = 6;
    pub(crate) const HEADERS: usize = 7;
    pub(crate) const COOKIES: usize = 8;
    pub(crate) const COOKIE_JAR: usize = 9;

    pub(crate) const fn empty() -> Self {
        Self::Fields(0)
    }

    pub(crate) const fn requires(self, field: usize) -> bool {
        match self {
            Self::Complete => true,
            Self::Fields(fields) => field < u16::BITS as usize && fields & (1_u16 << field) != 0,
        }
    }

    pub(crate) fn include(&mut self, field: usize) {
        if let Self::Fields(fields) = self {
            let Some(bit) = 1_u16.checked_shl(field as u32) else {
                *self = Self::Complete;
                return;
            };
            *fields |= bit;
        }
    }
}
