pub mod localfile;
pub mod parameters;
use crate::localfiles::localfile::Localfile;
use crate::localfiles::parameters::Parameters;

pub struct Localfiles {
    pub path: String,
    pub files: Vec<Localfile>,
    pub parameters: Parameters,
}

impl Localfiles {
    pub fn new(path: &String, parameters: Parameters) -> Self {
        let path = std::path::Path::new(&path);
        let mut files = Vec::new();

        if path.is_dir() {
            for entry in std::fs::read_dir(path).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                let file = Localfile::new(path);
                files.push(file);
            }
        } else {
            let file = Localfile::new(path.to_path_buf());
            files.push(file);
        }

        Self {
            path: path.to_str().unwrap().to_string(),
            files,
            parameters,
        }
    }
}
