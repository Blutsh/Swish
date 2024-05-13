use crate::api::chunks::{build_chunks_array, Chunk};
use crate::{
    api::{new_easy2_download, new_easy2_upload, post},
    errors::SwishError,
};
use serde_json::json;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::{fmt, path::Path};

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
    pub fn new(path: std::path::PathBuf, container: &serde_json::Value) -> Self {
        let path = path.clone();
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        let size = path.metadata().unwrap().len();
        let chunks = build_chunks_array(size as usize, CHUNK_SIZE);
        let container_uuid = container["container"]["UUID"].as_str().unwrap().to_string();
        let files_uuid = container["filesUUID"][0].as_str().unwrap().to_string();
        let upload_host = container["uploadHost"].as_str().unwrap().to_string();

        // we miglht need to check if the file exists here idk
        Self {
            path,
            name,
            size,
            upload_host,
            container_uuid,
            files_uuid,
            chunks,
        }
    }

    pub fn upload(&self) -> Result<(), SwishError> {
        let file = File::open(&self.path)?;
        let mut easy2 = new_easy2_upload("".to_string(), None, self.size as usize, &file)?; // Pass a reference to file

        // Iterate over a reference to chunks to avoid moving it
        for chunk in &self.chunks {
            let mut file = File::open(&self.path)?;
            file.seek(SeekFrom::Start(chunk.offset as u64))?;
            let mut buffer = vec![0; chunk.size];
            file.read_exact(&mut buffer)?;
            let upload_url = self.build_chunked_upload_url(&chunk);
            easy2.url(&upload_url)?;
            easy2.post(true)?;
            easy2.post_field_size(chunk.size as u64)?;
            easy2.perform()?;
        }
        Ok(())
    }

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
