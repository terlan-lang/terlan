use super::{VmHttpStaticAsset, VmHttpStaticError};

/// One validated byte-range request for a static asset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmHttpByteRange {
    Inclusive { start: usize, end: usize },
    From { start: usize },
    Suffix { length: usize },
}

#[cfg(test)]
impl VmHttpByteRange {
    /// Creates an inclusive byte range such as bytes 10 through 19.
    pub(crate) fn inclusive(start: usize, end: usize) -> Result<Self, VmHttpStaticError> {
        if start > end {
            return Err(VmHttpStaticError::InvalidRange);
        }
        Ok(Self::Inclusive { start, end })
    }

    /// Creates an open-ended byte range from `start` through the asset end.
    pub(crate) fn from(start: usize) -> Self {
        Self::From { start }
    }

    /// Creates a suffix range containing at most the final `length` bytes.
    pub(crate) fn suffix(length: usize) -> Result<Self, VmHttpStaticError> {
        if length == 0 {
            return Err(VmHttpStaticError::InvalidRange);
        }
        Ok(Self::Suffix { length })
    }

    /// Resolves a validated range against a concrete asset length.
    fn resolve(self, total: usize) -> Result<(usize, usize), VmHttpStaticError> {
        if total == 0 {
            return Err(VmHttpStaticError::UnsatisfiableRange);
        }
        match self {
            Self::Inclusive { start, end } if start < total => Ok((start, end.min(total - 1))),
            Self::From { start } if start < total => Ok((start, total - 1)),
            Self::Suffix { length } => Ok((total.saturating_sub(length), total - 1)),
            Self::Inclusive { .. } | Self::From { .. } => {
                Err(VmHttpStaticError::UnsatisfiableRange)
            }
        }
    }
}

impl VmHttpStaticAsset {
    /// Builds a deterministic single-range `206 Partial Content` response.
    #[cfg(test)]
    pub(crate) fn range_http_response(
        &self,
        range: VmHttpByteRange,
    ) -> Result<::http::Response<Vec<u8>>, VmHttpStaticError> {
        let total = self.bytes.len();
        let (start, end) = range.resolve(total)?;
        let body = self.bytes[start..=end].to_vec();
        ::http::Response::builder()
            .status(::http::StatusCode::PARTIAL_CONTENT)
            .header(::http::header::ACCEPT_RANGES, "bytes")
            .header(
                ::http::header::CONTENT_RANGE,
                format!("bytes {start}-{end}/{total}"),
            )
            .header(::http::header::CONTENT_LENGTH, body.len().to_string())
            .header(::http::header::CONTENT_TYPE, self.content_type())
            .header(::http::header::CACHE_CONTROL, self.cache_control())
            .body(body)
            .map_err(|_| VmHttpStaticError::InvalidResponse)
    }
}
