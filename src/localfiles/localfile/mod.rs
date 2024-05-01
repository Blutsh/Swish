pub struct Localfile {
    pub name: String,
    pub size: u64,
}

impl Localfile {
    pub fn new(path: std::path::PathBuf) -> Self {
        let metadata = std::fs::metadata(&path).unwrap();
        Self {
            name: path.file_name().unwrap().to_str().unwrap().to_string(),
            size: metadata.len(),
        }
    }
}
