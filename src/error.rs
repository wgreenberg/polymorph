use deku::DekuError;
use miniz_oxide::inflate::DecompressError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to make HTTP request")]
    HTTPRequestError(#[from] reqwest::Error),
    #[error("I/O error")]
    IOError(#[from] std::io::Error),
    #[error("Deku parsing error")]
    DekuError(#[from] DekuError),
    #[error("Missing CKey")]
    MissingCKey,
    #[error("Invalid Zlib")]
    ZlibError(DecompressError),
    #[error("Couldn't find file id {0}")]
    MissingFileId(u32),
    #[error("Couldn't find file with path {0}")]
    MissingFilePath(String),
}
