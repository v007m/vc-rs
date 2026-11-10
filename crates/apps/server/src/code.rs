//! The Connect protocol's error codes and the HTTP status a unary error is sent with.

/// Why a call failed, as the Connect protocol names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Code {
    /// The client cancelled the call.
    Canceled,
    /// No other code fits.
    Unknown,
    /// The request is invalid whatever the server's state.
    InvalidArgument,
    /// The deadline passed before the call completed.
    DeadlineExceeded,
    /// What the request names does not exist.
    NotFound,
    /// What the request would create exists already.
    AlreadyExists,
    /// The caller may not do this.
    PermissionDenied,
    /// A limit was reached, such as the size of a message.
    ResourceExhausted,
    /// The server is not in the state the call needs.
    FailedPrecondition,
    /// The call conflicted with another change, such as a file edited since it was read.
    Aborted,
    /// A value is outside its valid range.
    OutOfRange,
    /// The server does not serve this method.
    Unimplemented,
    /// Something inside the server failed; a bug to report.
    Internal,
    /// The server cannot serve the call now; it may be retried.
    Unavailable,
    /// Data was lost or corrupted.
    DataLoss,
    /// The call carries no valid credentials.
    Unauthenticated,
}

impl Code {
    /// The code's name on the wire, such as `invalid_argument`, and the HTTP status a unary
    /// call that fails with it is answered with.
    #[must_use]
    pub const fn wire(self) -> (&'static str, u16) {
        match self {
            Self::Canceled => ("canceled", 499),
            Self::Unknown => ("unknown", 500),
            Self::InvalidArgument => ("invalid_argument", 400),
            Self::DeadlineExceeded => ("deadline_exceeded", 504),
            Self::NotFound => ("not_found", 404),
            Self::AlreadyExists => ("already_exists", 409),
            Self::PermissionDenied => ("permission_denied", 403),
            Self::ResourceExhausted => ("resource_exhausted", 429),
            Self::FailedPrecondition => ("failed_precondition", 400),
            Self::Aborted => ("aborted", 409),
            Self::OutOfRange => ("out_of_range", 400),
            Self::Unimplemented => ("unimplemented", 501),
            Self::Internal => ("internal", 500),
            Self::Unavailable => ("unavailable", 503),
            Self::DataLoss => ("data_loss", 500),
            Self::Unauthenticated => ("unauthenticated", 401),
        }
    }
}
