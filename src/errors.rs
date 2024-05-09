use curl::Error as CurlError;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum SwishError {
    InvalidUrl { url: String },
    InvalidJson { json: String },
    InvalidResponse { response: String },
    CurlError { error: CurlError },
    FileError { error: std::io::Error },
    NotFound { url: String },
    PasswordRequired,
    InvalidPassword,
    DownloadNumberExceeded,
}

impl fmt::Display for SwishError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SwishError::InvalidUrl { url } => write!(f, "Invalid URL: {}", url),
            SwishError::InvalidJson { json } => write!(f, "Invalid JSON: {}", json),
            SwishError::NotFound { url } => write!(f, "Not Found: {}, Maybe link has expired", url),
            SwishError::CurlError { error } => write!(f, "Curl Error: {}", error),
            SwishError::InvalidResponse { response } => write!(f, "Invalid Response: {}", response),
            SwishError::FileError { error } => write!(f, "File Error: {}", error),
            SwishError::PasswordRequired => write!(f, "A password is required to download this file please provide it using the -p flag or --password flag"),
            SwishError::InvalidPassword => write!(f, "The password provided is incorrect"),
            SwishError::DownloadNumberExceeded => write!(f, "The number of download has been exceeded"),
        }
    }
}

impl Error for SwishError {}

impl From<CurlError> for SwishError {
    fn from(error: CurlError) -> SwishError {
        SwishError::CurlError { error }
    }
}

impl From<std::io::Error> for SwishError {
    fn from(error: std::io::Error) -> SwishError {
        SwishError::FileError { error }
    }
}

impl From<serde_json::Error> for SwishError {
    fn from(error: serde_json::Error) -> SwishError {
        SwishError::InvalidJson {
            json: error.to_string(),
        }
    }
}
