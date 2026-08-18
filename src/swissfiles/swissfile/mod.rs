use crate::api::chunks::{build_chunks_array, Chunk};
use crate::{
    api::{new_easy2_download, post},
    errors::SwishError,
};
use indicatif::{ProgressBar, ProgressStyle};
use serde_json::json;
use std::fmt;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const SWISSTRANSFER_API: &str = "https://www.swisstransfer.com/api";
const CHUNK_SIZE: usize = 52428800;

pub enum Swissfile {
    Local(LocalSwissfile),
    Remote(RemoteSwissfile),
}

pub struct LocalSwissfile {
    pub path: std::path::PathBuf,
    pub name: String,
    pub size: u64,
    pub upload_host: String,
    pub container_uuid: String,
    pub files_uuid: String,
    pub chunks: Vec<Chunk>,
}

impl fmt::Display for LocalSwissfile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Name: {}, Size: {}", self.name, self.size)
    }
}

impl LocalSwissfile {
    pub fn new(path: std::path::PathBuf, container: &serde_json::Value, file_uuid: String) -> Self {
        let path = path.clone();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let size = path.metadata().map(|m| m.len()).unwrap_or(0);
        let chunks = build_chunks_array(size as usize, CHUNK_SIZE);
        let container_uuid = container["container"]["UUID"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let upload_host = container["uploadHost"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        Self {
            path,
            name,
            size,
            upload_host,
            container_uuid,
            files_uuid: file_uuid,
            chunks,
        }
    }

    // Add back the build_chunked_upload_url method
    fn build_chunked_upload_url(&self, chunk: &Chunk) -> String {
        format!(
            "https://{}/api/uploadChunk/{}/{}/{}/{}",
            self.upload_host,
            self.container_uuid,
            self.files_uuid,
            chunk.index,
            if chunk.index == self.chunks.len() - 1 {
                "1"
            } else {
                "0"
            }
        )
    }

    pub fn upload(&self) -> Result<(), SwishError> {
        log::debug!("Uploading file: {} (UUID: {})", self.name, self.files_uuid);

        // Create a single progress bar for the entire file
        let progress_bar = ProgressBar::new(self.size);
        progress_bar.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})").unwrap()
        .progress_chars("#>-"));

        // Share this progress bar across all chunk uploads
        let progress = Arc::new(Mutex::new(progress_bar));

        // We want to upload each chunk in sequence
        for (i, chunk) in self.chunks.iter().enumerate() {
            log::debug!(
                "Uploading chunk {}/{} of file {}",
                i + 1,
                self.chunks.len(),
                self.name
            );

            // Open file and seek to chunk position
            let mut file = File::open(&self.path)?;
            file.seek(SeekFrom::Start(chunk.offset as u64))?;

            // Create the upload URL for this chunk
            let upload_url = self.build_chunked_upload_url(&chunk);
            log::debug!("Chunk upload URL: {}", upload_url);

            // Create a buffer for just this chunk
            let mut buffer = vec![0u8; chunk.size];
            file.read_exact(&mut buffer)?;

            // Create a curl easy object
            let mut easy = curl::easy::Easy::new();
            easy.url(&upload_url)?;
            easy.upload(true)?;
            easy.post(true)?;

            // Important: Set the correct content length for this chunk
            easy.post_field_size(chunk.size as u64)?;

            // Setup the headers
            let mut list = curl::easy::List::new();
            list.append("User-Agent: swisstransfer-webext/1.0")?;
            list.append("Cookie: webext=1")?;
            list.append("Referer: swissfer/2.0.0")?;
            list.append("Content-Type: application/octet-stream")?;
            easy.http_headers(list)?;

            // Write the data using the shared progress bar
            {
                let mut data = buffer.as_slice();
                let progress_clone = Arc::clone(&progress);

                let mut transfer = easy.transfer();
                transfer.read_function(move |into| {
                    let amount = std::cmp::min(into.len(), data.len());
                    if amount == 0 {
                        return Ok(0);
                    }

                    into[..amount].copy_from_slice(&data[..amount]);

                    // Update the shared progress bar
                    progress_clone.lock().unwrap().inc(amount as u64);

                    data = &data[amount..];
                    Ok(amount)
                })?;
                transfer.perform()?;
            }

            // Check response
            let response_code = easy.response_code()?;
            if response_code >= 400 {
                return Err(SwishError::InvalidResponse {
                    response: format!(
                        "Failed to upload chunk {}/{} of file {} with status code: {}",
                        i + 1,
                        self.chunks.len(),
                        self.name,
                        response_code
                    ),
                });
            }
        }

        // Make sure the progress bar is marked as finished
        progress.lock().unwrap().finish();

        Ok(())
    }
}

pub struct RemoteSwissfile {
    pub name: String,
    pub size: u64,
    pub url: String,
    pub created_date: String,
    pub expired_date: String,
    pub deleted_date: String,
    pub download_counter: u64,
    pub e_virus_scan: String,
    pub mime_type: String,
    pub uuid: String,
    pub download_base_url: String,
    pub container_uuid: String,
    pub password: Option<String>,
}

impl fmt::Display for RemoteSwissfile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Name: {}, Size: {}, URL: {}, Created: {}, Expired: {}, Mime: {}",
            self.name, self.size, self.url, self.created_date, self.expired_date, self.mime_type
        )
    }
}

impl RemoteSwissfile {
    pub fn new(
        json: &serde_json::Value,
        download_base_url: &str,
        container_uuid: &str,
        password: Option<&str>,
    ) -> Self {
        let container_uuid = container_uuid.to_string();
        let uuid = json["UUID"].as_str().unwrap().to_string();

        // If file is password protected, generate a download token and build the URL accordingly
        let url = match password {
            Some(ref password) => {
                let token =
                    RemoteSwissfile::generate_download_token(password, &container_uuid, &uuid)
                        .unwrap();
                let token: String =
                    serde_json::from_str(token.as_str()).unwrap_or_else(|_| token.to_string());
                format!("{}/{}?token={}", download_base_url, uuid, token)
            }
            None => format!(
                "{}/{}",
                download_base_url,
                json["UUID"].as_str().unwrap().to_string()
            ),
        };

        Self {
            name: json["fileName"].as_str().unwrap().to_string(),
            size: json["fileSizeInBytes"].as_u64().unwrap(),
            url,
            created_date: json["createdDate"].as_str().unwrap().to_string(),
            expired_date: json["expiredDate"].as_str().unwrap().to_string(),
            deleted_date: "test".to_owned(), // json["deletedDate"].as_str().unwrap().to_string(),
            download_counter: json["downloadCounter"].as_u64().unwrap(),
            e_virus_scan: json["eVirus"].as_str().unwrap().to_string(),
            mime_type: json["mimeType"].as_str().unwrap().to_string(),
            uuid,
            download_base_url: download_base_url.to_string(),
            container_uuid,
            password: password.map(|s| s.to_string()),
        }
    }

    fn generate_download_token(
        password: &str,
        container_uuid: &str,
        file_uuid: &str,
    ) -> Result<String, SwishError> {
        log::debug!("Generating download token for file: {}", file_uuid);
        let url = format!("{}/generateDownloadToken", SWISSTRANSFER_API);
        let payload = json!({
            "password": password,
            "containerUUID": container_uuid,
            "fileUUID": file_uuid,
        });

        let response = post(url.as_str(), payload.to_string().into_bytes(), None)?;
        let token: String = String::from_utf8(response).unwrap();

        log::debug!("Retrieved Token : {:?}", token);

        Ok(token)
    }

    pub fn download(&self, custom_out_path: Option<&PathBuf>) -> Result<(), SwishError> {
        log::debug!("Downloading {} from {}", self.name, self.url.clone());
        // Dereference the PathBuf if it exists
        let out_path = match custom_out_path {
            Some(path) => path.join(&self.name),
            None => PathBuf::from(".").join(&self.name),
        };

        let out_path = out_path.to_str().unwrap();
        let file = std::fs::File::create(&out_path)?;
        let url = self.url.clone();

        let easy2 = new_easy2_download(url, None, file, self.size)?;
        easy2.perform()?;

        match easy2.response_code()? {
            500 => {
                // Clean up the file as it is invalid anyway
                std::fs::remove_file(&out_path)?;

                // we are not sure but we can assume that this is the error x)
                Err(SwishError::DownloadNumberExceeded)
            }
            _ => Ok(()),
        }
    }
}

impl fmt::Display for Swissfile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Swissfile::Local(local_swissfile) => write!(f, "{}", local_swissfile),
            Swissfile::Remote(remote_swissfile) => write!(f, "{}", remote_swissfile),
        }
    }
}
