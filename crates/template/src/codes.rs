//! The diagnostic code for each kind of problem this crate reports, in one place.

use vc_diagnostics::Code;

/// A template that does not parse (D144).
pub const TEMPLATE_SYNTAX: Code = Code::V2305;
/// An unknown pipe, or a pipe with the wrong argument count or types (D145).
pub const PIPE_CALL: Code = Code::V2308;
/// A literal regex given to CEL's `matches()` that does not compile.
pub const MATCHER_ARG: Code = Code::V2306;
/// A pipe that cannot take the type of value it is given (D145).
pub const PIPE_TYPE: Code = Code::V2304;
/// A root not available at that spot (D125).
pub const ROOT_UNAVAILABLE: Code = Code::V2105;
/// `${item}`, `${example}` or `${iteration}` outside its block (D137).
pub const LOOP_ROOT: Code = Code::V2102;
/// An invalid duration or size.
pub const UNIT: Code = Code::V2301;
/// An invalid status code or class.
pub const STATUS: Code = Code::V2303;
