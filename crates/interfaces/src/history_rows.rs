//! The rows of a test instance's result that the history stores: its checks, exchanges and
//! their timings (D170).

use vc_model::TestStatus;

/// A test instance's result.
#[derive(Clone, Debug, Default, PartialEq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "history row built and read across crates; every user is in this repo (D160)"
)]
pub struct TestRow {
    /// Its full ID: `users.crud#getsUser`.
    pub test_id: String,
    /// Its `examples` row, from 0.
    pub example_row: Option<u32>,
    /// Its repeat, from 0.
    pub repeat_index: u32,
    /// Its label as the run printed it, masked (D316): `users.crud#getsUser › Oslo [repeat 2]`;
    /// `None` in a row stored before labels were.
    pub label: Option<String>,
    /// How it ended.
    pub status: Option<TestStatus>,
    /// Its wall time, in milliseconds.
    pub duration_ms: f64,
    /// How often it ran.
    pub attempts: u32,
    /// The lifecycle action whose failure blocked it.
    pub blocked_by: Option<String>,
    /// Why it was skipped.
    pub skip_reason: Option<String>,
    /// Its failure's message.
    pub message: Option<String>,
    /// Its checks, in order.
    pub checks: Vec<CheckRow>,
    /// Its exchanges, in order.
    pub exchanges: Vec<ExchangeRow>,
}

/// One check of a test.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "history row built and read across crates; every user is in this repo (D160)"
)]
pub struct CheckRow {
    /// Its label: `status`, `body.match.total`.
    pub kind: String,
    /// The path it checked, if any.
    pub path: Option<String>,
    /// The expected value, as JSON.
    pub expected: String,
    /// The actual value, as JSON, if there was one.
    pub actual: Option<String>,
    /// Whether it held.
    pub passed: bool,
    /// Who wrote it: `api users#get`, `test`, `action`.
    pub origin: String,
    /// The file it is written in.
    pub file: String,
    /// The line it is written on.
    pub line: u32,
}

/// A request and its response, redacted, bodies cut.
#[derive(Clone, Debug, Default, PartialEq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "history row built and read across crates; every user is in this repo (D160)"
)]
pub struct ExchangeRow {
    /// The action that sent it.
    pub action: String,
    /// The api it called, if it called one.
    pub api_id: Option<String>,
    /// The request method.
    pub method: String,
    /// The request URL.
    pub url: String,
    /// The response status.
    pub status: Option<u16>,
    /// Its timings.
    pub timing: TimingRow,
    /// The request headers, in order.
    pub request_headers: Vec<(String, String)>,
    /// The response headers, in order.
    pub response_headers: Vec<(String, String)>,
    /// The request body, when `history.bodies` keeps it.
    pub request_body: Option<Vec<u8>>,
    /// The response body, when `history.bodies` keeps it.
    pub response_body: Option<Vec<u8>>,
    /// Whether a body was cut at `history.maxBodySize`.
    pub body_truncated: bool,
}

/// An exchange's timings, in milliseconds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "history row built and read across crates; every user is in this repo (D160)"
)]
pub struct TimingRow {
    /// The DNS lookup, when there was one.
    pub dns_ms: Option<f64>,
    /// The TCP connect, when there was one.
    pub connect_ms: Option<f64>,
    /// The TLS handshake, when there was one.
    pub tls_ms: Option<f64>,
    /// Until the first response byte.
    pub first_byte_ms: f64,
    /// Reading the body.
    pub download_ms: f64,
    /// All of it.
    pub total_ms: f64,
}

/// What a load run found: a threshold it breached or a kind of error it hit (D322).
#[derive(Clone, Debug, Default, PartialEq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "history row built and read across crates; every user is in this repo (D160)"
)]
pub struct FindingRow {
    /// A breach or an error.
    pub kind: FindingKind,
    /// The test it belongs to, when it belongs to one.
    pub test_id: Option<String>,
    /// The api an error came from, when it came from one.
    pub api_id: Option<String>,
    /// A breached threshold's metric: `p95`, `errorRate`.
    pub metric: Option<String>,
    /// An error's response status, when it had a response.
    pub status: Option<u16>,
    /// An error's diagnostic code, when it had one.
    pub code: Option<String>,
    /// How many times an error happened.
    pub count: Option<u64>,
    /// A breached metric's value, in its unit.
    pub actual: Option<f64>,
    /// The line the run printed for it, masked; never a raw value.
    pub shown: String,
}

/// Whether a [`FindingRow`] is a breached threshold or a kind of error.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_enums,
    reason = "history row built and read across crates; every user is in this repo (D160)"
)]
pub enum FindingKind {
    /// A threshold the run breached.
    #[default]
    Breach,
    /// A kind of error, counted.
    Error,
}

impl FindingKind {
    /// How the store writes it: `breach` or `error`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Breach => "breach",
            Self::Error => "error",
        }
    }
}
