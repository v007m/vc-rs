//! Why a load run could not be set up or judged.

/// Why the load engine refused a scenario or a recording.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LoadError {
    /// Every test in the mix has weight 0, so no iteration could ever be picked.
    #[error("the mix has no test with a weight above 0")]
    EmptyMix,
    /// The latency histogram could not be made or merged.
    #[error("the latency histogram failed: {0}")]
    Histogram(String),
    /// Bytes that are not an uncompressed `HdrHistogram` V2 encoding of a load histogram.
    #[error("the histogram encoding cannot be read: {0}")]
    BadEncoding(String),
    /// A threshold names a metric the engine does not measure, or its matcher cannot be used.
    #[error("the `{metric}` threshold cannot be judged: {message}")]
    Threshold {
        /// The metric as written.
        metric: String,
        /// What is wrong.
        message: String,
    },
}
