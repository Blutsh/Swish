use curl::Error as CurlError;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum SwishError {
    InvalidUrl { url: String },
    InavlidArg { arg: String },
    InvalidJson { json: String },
    InvalidResponse { response: String },
    InvalidFile { file: String },
    InvalidPath { path: String },
    CurlError { error: CurlError },
    FileError { error: std::io::Error },
    JSONError { error: serde_json::Error },
    NotFound { url: String },
    PasswordRequired,
    InvalidPassword,
}

impl fmt::Display for SwishError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SwishError::InvalidUrl { url } => write!(f, "Invalid URL: {}", url),
            SwishError::InvalidJson { json } => write!(f, "Invalid JSON: {}", json),
            SwishError::NotFound { url } => write!(f, "Not Found: {}, Maybe link has expired", url),
            SwishError::InvalidFile { file } => write!(f, "Invalid File: {}", file),
            SwishError::InvalidPath { path } => write!(f, "Invalid Path: {}", path),
            SwishError::CurlError { error } => write!(f, "Curl Error: {}", error),
            SwishError::InvalidResponse { response } => write!(f, "Invalid Response: {}", response),
            SwishError::FileError { error } => write!(f, "File Error: {}", error),
            SwishError::JSONError { error } => write!(f, "JSON Error: {}", error),
            SwishError::InavlidArg { arg } => write!(f, "Invalid Argument: {}", arg),
            SwishError::PasswordRequired => write!(f, "A password is required to download this file please provide it using the -p flag or --password flag"),
            SwishError::InvalidPassword => write!(f, "The password provided is incorrect"),
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
