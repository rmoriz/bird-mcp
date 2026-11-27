//! Error type shared by the transport, parsers and MCP layer.

use rmcp::ErrorData;

pub type ToolResult<T> = Result<T, ToolError>;

/// Every failure a tool can hit, with a message that tells the caller what to do next.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("{0}")]
    Config(String),

    #[error("unknown router '{name}' (configured: {known})")]
    UnknownRouter { name: String, known: String },

    #[error(
        "router '{router}' is read-only: {op} needs write access. Set `read_only = false` for \
         this router (BIRD_MCP_CONFIG or BIRD_MCP_ROUTERS) and restart bird-mcp."
    )]
    ReadOnly { router: String, op: String },

    #[error("{0}")]
    BadArgs(String),

    #[error("`{command}` failed on '{router}'{status}: {detail}")]
    CommandFailed {
        router: String,
        command: String,
        status: String,
        detail: String,
    },

    #[error("`{command}` on '{router}' timed out after {secs}s")]
    Timeout {
        router: String,
        command: String,
        secs: u64,
    },

    #[error("could not start `{command}`: {source}")]
    Spawn {
        command: String,
        #[source]
        source: std::io::Error,
    },

    #[error("docs unavailable: {0}")]
    Docs(String),

    #[error("{0}")]
    Io(String),
}

impl ToolError {
    pub fn bad_args(message: impl Into<String>) -> Self {
        ToolError::BadArgs(message.into())
    }

    pub fn into_mcp(self) -> ErrorData {
        match self {
            ToolError::BadArgs(_) | ToolError::UnknownRouter { .. } => {
                ErrorData::invalid_params(self.to_string(), None)
            }
            _ => ErrorData::internal_error(self.to_string(), None),
        }
    }
}

impl From<ToolError> for ErrorData {
    fn from(value: ToolError) -> Self {
        value.into_mcp()
    }
}

impl From<std::io::Error> for ToolError {
    fn from(value: std::io::Error) -> Self {
        ToolError::Io(value.to_string())
    }
}
