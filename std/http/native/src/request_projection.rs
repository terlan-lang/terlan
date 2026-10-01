//! HTTP ingress field selection derived from generic compiler observations.

/// Fields in the HTTP request snapshot that one AOT export may observe.
///
/// `Complete` is the fail-closed representation. `Fields` is emitted only when
/// the HTTP adapter accepts a non-escaping aggregate projection proof and maps
/// its observations to this ingress contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum RequestFieldProjection {
    Complete,
    Fields(u16),
}

impl RequestFieldProjection {
    pub const METHOD: usize = 1;
    pub const PATH: usize = 2;
    pub const PARAMS: usize = 3;
    pub const BODY: usize = 4;
    pub const QUERY_STRING: usize = 5;
    pub const QUERY: usize = 6;
    pub const HEADERS: usize = 7;
    pub const COOKIES: usize = 8;
    pub const BODY_FILE_PATH: usize = 9;

    pub const fn empty() -> Self {
        Self::Fields(0)
    }

    pub const fn requires(self, field: usize) -> bool {
        match self {
            Self::Complete => true,
            Self::Fields(fields) => field < u16::BITS as usize && fields & (1_u16 << field) != 0,
        }
    }

    pub fn include(&mut self, field: usize) {
        if let Self::Fields(fields) = self {
            let Some(bit) = u32::try_from(field)
                .ok()
                .and_then(|field| 1_u16.checked_shl(field))
            else {
                *self = Self::Complete;
                return;
            };
            *fields |= bit;
        }
    }
}
