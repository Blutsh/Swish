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
    TransferInfected,
    CountryBlocked,
    UnexpectedState { state: String },
    UploadBlocked,
    EmailRequired,
    EmailValidationExpired,
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
            SwishError::DownloadNumberExceeded => write!(f, "This transfer is no longer available, it has expired or reached its download limit"),
            SwishError::TransferInfected => write!(f, "Infomaniak's antivirus flagged this transfer as infected, so it cannot be downloaded"),
            SwishError::CountryBlocked => write!(f, "This transfer is not available from your country"),
            SwishError::UnexpectedState { state } => write!(f, "SwissTransfer returned an unexpected page state: {}. The site may have changed again", state),
            SwishError::UploadBlocked => write!(f, "Upload was rejected: SwissTransfer now protects transfer creation with an Altcha anti-bot challenge that this tool does not solve"),
            SwishError::EmailRequired => write!(f, "SwissTransfer requires a sender address to upload. Pass one with the -e flag or --email flag"),
            SwishError::EmailValidationExpired => write!(f, "The saved confirmation for this address is no longer valid"),
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
