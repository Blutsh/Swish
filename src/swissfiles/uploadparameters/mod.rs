pub struct UploadParameters {
    pub duration: u64,
    /// Sender address. SwissTransfer requires one, and requires it confirmed.
    pub author_email: Option<String>,
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
            author_email: None,
            password: "".to_owned(),
            message: "".to_owned(),
            number_of_download: 250,
            // The API only accepts a bare language code, not a locale.
            lang: "en".to_owned(),
            recipients_emails: Vec::new(),
        }
    }
}
