//! What can go wrong serving MCP or answering one tool call.

use serde_json::{Value, json};
use vc_engine::EngineError;

/// An error from the MCP server or one of its tools.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum McpError {
    /// The engine refused or failed the call.
    #[error(transparent)]
    Engine(#[from] EngineError),
    /// A tool argument is missing or has the wrong type.
    #[error("argument `{name}`: {reason}")]
    Argument {
        /// The argument's name.
        name: &'static str,
        /// What is wrong with it.
        reason: String,
    },
    /// A tool argument the tool's schema does not name.
    #[error("argument `{name}`: is not one this tool takes; it takes {takes}")]
    UnknownArgument {
        /// The argument's name.
        name: String,
        /// The arguments the tool takes, quoted and comma-separated, or `none`.
        takes: String,
    },
    /// Something inside the server failed; a bug to report.
    #[error("{reason}")]
    Internal {
        /// What went wrong.
        reason: String,
    },
    /// The client sent something other than `initialize` first, which the protocol does not
    /// allow: the client's fault, not the server's.
    #[error("the MCP client broke the handshake: it sent {sent} before an `initialize` was answered")]
    Handshake {
        /// What it sent: a method in backticks, `a response` or `nothing`.
        sent: String,
    },
    /// The MCP session could not start or ended with an error.
    #[error("the MCP server stopped: {reason}")]
    Serve {
        /// What went wrong.
        reason: String,
    },
}

impl McpError {
    /// The error as a tool result an agent can act on: `kind` is `refused` when the agent
    /// policy or a person said no, `arguments` for a bad argument and `error` otherwise.
    #[must_use]
    pub fn refusal(&self) -> Value {
        let (kind, help) = match self {
            Self::Engine(error @ EngineError::Refused { .. }) => ("refused", error.help()),
            Self::Engine(error) => ("error", error.help()),
            Self::Argument { .. } | Self::UnknownArgument { .. } => ("arguments", None),
            Self::Internal { .. } | Self::Handshake { .. } | Self::Serve { .. } => ("error", None),
        };
        json!({ "kind": kind, "reason": self.to_string(), "help": help })
    }
}
