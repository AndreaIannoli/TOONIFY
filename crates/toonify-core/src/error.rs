use std::fmt;

use crate::input::SourceFormat;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ToonifyError {
    #[error("failed to read input: {0}")]
    Io(#[from] std::io::Error),
    #[error("{format:?} parsing error: {message}")]
    Parse {
        format: SourceFormat,
        message: String,
    },
    #[error("number normalization error for `{value}`: {source}")]
    NumberNormalization {
        value: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("XML decoding error: {0}")]
    Xml(String),
    #[error("{0}")]
    Encoding(String),
    #[error("{0}")]
    Decoding(String),
}

impl ToonifyError {
    pub(crate) fn parse_err(
        format: SourceFormat,
        err: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Parse {
            format,
            message: err.to_string(),
        }
    }

    pub(crate) fn encoding(msg: impl fmt::Display) -> Self {
        Self::Encoding(msg.to_string())
    }

    pub(crate) fn decoding(msg: impl fmt::Display) -> Self {
        Self::Decoding(msg.to_string())
    }
}
