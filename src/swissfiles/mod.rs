use std::fmt;
pub mod swissfile;
use swissfile::Swissfile;

pub struct Swissfiles {
    pub files: Vec<Swissfile>,
    download_base_url: String,
    pub container_uuid: String,
    pub need_password: bool,
}

impl Swissfiles {
    pub fn new(json: &serde_json::Value) -> Self {
        log::debug!("Swissfiles::new: {}", json);
        let download_host = json["data"]["downloadHost"].as_str().unwrap().to_string();
        let link_uuid = json["data"]["linkUUID"].as_str().unwrap().to_string();
        let download_base_url = format!("https://{}/api/download/{}", &download_host, &link_uuid);
        let need_password = json["data"]["container"]["needPassword"].as_i64().unwrap() == 1;
        let container_uuid = json["data"]["container"]["UUID"].as_str().unwrap().to_string();

        let mut files = Vec::new();
        for file in json["data"]["container"]["files"].as_array().unwrap() {
            files.push(Swissfile::new(file, &download_base_url));
        }
        Self {
            files,
            download_base_url,
            container_uuid,
            need_password,
        }
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
