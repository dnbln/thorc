use std::io;

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum GetIndexError {
    #[error("download error: {0}")]
    Download(#[from] DownloadError),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("deserialization error: {0}")]
    DeserializeError(#[from] toml::de::Error),
}

#[derive(thiserror::Error, Debug)]
#[error("no such git provider")]
pub struct NoSuchGitProviderError;


#[derive(Debug, thiserror::Error)]
pub enum CheckTemplateNameError {
    #[error("invalid character {c:?} at {index}")]
    InvalidCharacter { c: char, index: usize },
}
