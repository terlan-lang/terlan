/// Portable HTTP adapter error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpError {
    code: &'static str,
    message: String,
    status: i64,
}

impl HttpError {
    /// Builds a portable HTTP adapter error.
    ///
    /// Inputs:
    /// - `code`: stable machine-readable error code.
    /// - `message`: human-readable diagnostic text.
    /// - `status`: HTTP status most closely associated with the failure.
    ///
    /// Output:
    /// - `HttpError` with stable fields.
    ///
    /// Transformation:
    /// - Converts runtime or parser failures into the shared HTTP error shape.
    pub fn new(code: &'static str, message: impl Into<String>, status: i64) -> Self {
        Self {
            code,
            message: message.into(),
            status,
        }
    }

    /// Returns the stable machine-readable error code.
    ///
    /// Inputs:
    /// - `self`: HTTP error value.
    ///
    /// Output:
    /// - Static error code string.
    ///
    /// Transformation:
    /// - Reads the code field without allocation or mutation.
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// Returns the human-readable error message.
    ///
    /// Inputs:
    /// - `self`: HTTP error value.
    ///
    /// Output:
    /// - Borrowed message text.
    ///
    /// Transformation:
    /// - Reads the message field without allocation or mutation.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the HTTP status associated with this error.
    ///
    /// Inputs:
    /// - `self`: HTTP error value.
    ///
    /// Output:
    /// - Numeric HTTP status code.
    ///
    /// Transformation:
    /// - Reads the status field without allocation or mutation.
    pub fn status(&self) -> i64 {
        self.status
    }
}
