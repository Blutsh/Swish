pub struct UploadParameters {
    pub duration: u64,
    pub author_email: String,
    pub password: String,
    pub message: String,
    pub number_of_download: u16,
    pub lang: String,
    pub recipients_emails: Vec<String>,
}

impl Default for UploadParameters {
    fn default() -> Self {
        Self {
            duration: 30,
            author_email: "".to_owned(),
            password: "".to_owned(),
            message: "".to_owned(),
            number_of_download: 250,
            lang: "en_GB".to_owned(),
            recipients_emails: Vec::new(),
        }
    }
}