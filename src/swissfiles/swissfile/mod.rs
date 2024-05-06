use std::{fmt, path::Path};
use serde_json::json;
use crate::{api::{new_easy2_download, post}, errors::SwishError};

const SWISSTRANSFER_API: &str = "https://www.swisstransfer.com/api";

pub struct Swissfile {
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

impl fmt::Display for Swissfile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Name: {}, Size: {}, URL: {}, Created: {}, Expired: {}, Mime: {}",
            self.name, self.size, self.url, self.created_date, self.expired_date, self.mime_type
        )
    }
}

impl Swissfile {
    pub fn new(json: &serde_json::Value, download_base_url: &str, container_uuid: &str, password: Option<&str>) -> Self {
        let container_uuid = container_uuid.to_string();
        let uuid = json["UUID"].as_str().unwrap().to_string();

        // If file is password protected, generate a download token and build the URL accordingly
        let url = match password {
            Some(ref password) => {
                let token = Swissfile::generate_download_token(password, &container_uuid, &uuid).unwrap();
                let token: String =
                    serde_json::from_str(token.as_str()).unwrap_or_else(|_| token.to_string());
                format!(
                    "{}/{}?token={}",
                    download_base_url,
                    uuid,
                    token
                )
            }
            None => format!(
                "{}/{}",
                download_base_url,
                json["UUID"].as_str().unwrap().to_string()
            )
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

    fn generate_download_token(password: &str, container_uuid: &str, file_uuid: &str) -> Result<String, SwishError> {
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


    pub fn download(&self, custom_out_path: Option<&Path>) -> Result<(), SwishError> {
        log::debug!("Downloading {} from {}",  self.name, self.url.clone());
        let out_path = custom_out_path.unwrap_or_else(|| Path::new(".")).join(&self.name);
        let out_path = out_path.to_str().unwrap();
        let file = std::fs::File::create(&out_path)?;
        let url = self.url.clone(); 
        
        let easy2 = new_easy2_download(url, None, file)?;
        easy2.perform()?;

        Ok(())
    }
}
