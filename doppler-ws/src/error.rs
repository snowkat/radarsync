use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error("{0}")]
    Websocket(#[from] tokio_websockets::Error),
    #[error("Unexpected response from server")]
    MalformedResponse,
    #[error("Got unexpected {0} response from server")]
    BadResponse(http::StatusCode),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error("Received pairing request from unexpected device")]
    UnexpectedDevice,
    #[error("Device object is missing ID")]
    DeviceIdMissing,
    #[error("Error parsing URL: {0}")]
    InvalidUrl(#[from] url::ParseError),
    #[error("The provided path was invalid")]
    InvalidPath,
}
