use std::{fmt, path::Path};

mod swissfile;
use crate::{api::get, errors::SwishError, swissfiles::swissfile::Swissfile};
use base64::prelude::*;

const SWISSTRANSFER_API: &str = "https://www.swisstransfer.com/api";

pub struct Swissfiles {
    pub files: Vec<Swissfile>,
}

impl Swissfiles {
    pub fn new(swisstransfer_link: &str, password: Option<&str>) -> Result<Self, SwishError> {
        log::debug!("Creating new swissfiles : {}", &swisstransfer_link);

        // We might verify link validity there idk

        let download_id = swisstransfer_link.split("/").last().unwrap();
        let url = format!("{}/links/{}", SWISSTRANSFER_API, download_id);

        // if password is provided, add authorization header
        let auth_header: Option<Vec<String>> = match password {
            Some(password) => {
                // Encode password to base64
                let password = BASE64_STANDARD.encode(password);

                // Add authorization header
                let header = "Authorization: ".to_string() + &password;

                log::debug!(
                    "Password has been provided. Adding authorization header : {}",
                    &header
                );
                Some(vec![header])
            }
            None => None,
        };

        let response = get(&url, auth_header.clone())?;
        let response: serde_json::Value = serde_json::from_str(&response)?;

        //  Handling different responses
        match response["data"]["message"].as_str() {
            Some("Transfer need a password") => return Err(SwishError::PasswordRequired),
            Some("The password is wrong") => return Err(SwishError::InvalidPassword),
            Some("All file are not finish to virus check") => {
                loop {
                    // Wait for security checks on Infomaniak's side
                    print!("Waiting for security checks on Infomaniak's side");
                    std::thread::sleep(std::time::Duration::from_secs(5));
                    let response = get(&url, auth_header.clone())?;
                    let response: serde_json::Value = serde_json::from_str(&response)?;

                    let message = response["data"]["message"].as_str();

                    if message == Some("All file are finish to virus check") {
                        break;
                    }
                }
            }
            _ => (),
        }

        //Retrieve information from the resposne
        let download_host = response["data"]["downloadHost"]
            .as_str()
            .unwrap()
            .to_string();
        let link_uuid = response["data"]["linkUUID"].as_str().unwrap().to_string();
        let container_uuid = response["data"]["container"]["UUID"]
            .as_str()
            .unwrap()
            .to_string();

        // Build the download base url
        let download_base_url = format!("https://{}/api/download/{}", &download_host, &link_uuid);

        let mut files = Vec::new();

        for file in response["data"]["container"]["files"].as_array().unwrap() {
            let swissfile = Swissfile::new(file, &download_base_url, &container_uuid, password);
            files.push(swissfile);
        }

        Ok(Self { files })
    }

    pub fn download(&self, path: Option<&Path>) -> Result<(), crate::errors::SwishError> {
        for file in &self.files {
            file.download(path)?;
        }
        Ok(())
    }
}

impl fmt::Display for Swissfiles {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for file in &self.files {
            write!(f, "{}\n", file)?;
        }
        Ok(())
    }
}
