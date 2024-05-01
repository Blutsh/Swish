use std::fmt;

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
    pub fn new(json: &serde_json::Value, download_base_path: &str) -> Self {
        Self {
            name: json["fileName"].as_str().unwrap().to_string(),
            size: json["fileSizeInBytes"].as_u64().unwrap(),
            url: format!(
                "{}/{}",
                &download_base_path,
                json["UUID"].as_str().unwrap().to_string()
            ),
            created_date: json["createdDate"].as_str().unwrap().to_string(),
            expired_date: json["expiredDate"].as_str().unwrap().to_string(),
            deleted_date: "test".to_owned(), // json["deletedDate"].as_str().unwrap().to_string(),
            download_counter: json["downloadCounter"].as_u64().unwrap(),
            e_virus_scan: json["eVirus"].as_str().unwrap().to_string(),
            mime_type: json["mimeType"].as_str().unwrap().to_string(),
        }
    }
}
