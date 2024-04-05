use deku::DekuError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[cfg(feature = "reqwest")]
    #[error("Failed to make HTTP request")]
    HTTPRequestError(#[from] reqwest::Error),
    #[error("I/O error")]
    IOError(#[from] std::io::Error),
    #[error("Deku parsing error")]
    DekuError(#[from] DekuError),
    #[error("Missing CKey")]
    MissingCKey,
    #[cfg(feature = "tact")]
    #[error("Invalid Zlib")]
    ZlibError(miniz_oxide::inflate::DecompressError),
    #[error("Couldn't find file id {0}")]
    MissingFileId(u32),
    #[error("Couldn't find file with path {0}")]
    MissingFileName(String),
    #[error("BLTE for file contains an encrypted frame, which we don't support")]
    UnsupportedEncryptedData,
}
