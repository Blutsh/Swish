use swish::api::upload;
use swish::api::download;
use swish::localfiles;
use swish::swissfiles::Swissfiles;
use swish::swissfiles::swissfile::Swissfile;

const TEST_FILE_BASEPATH: &str = "tests/file_samples/";
const TEST_FILE_DOWNLOADED_BASEPATH: &str = "tests/downloaded_files/";

fn hash_file(file_path: &str) -> String {
    use std::fs::File;
    use std::io::{BufReader, Read};
    use sha2::{Digest, Sha256};

    let file = File::open(file_path).unwrap();
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();

    let mut buffer = [0; 1024];
    loop {
        let n = reader.read(&mut buffer).unwrap();
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    format!("{:x}", hasher.finalize())
}

#[test]

fn test_single_file_upload_download(){

    //take the first file in the file_samples folder
    let file_path = std::fs::read_dir(TEST_FILE_BASEPATH).unwrap().next().unwrap().unwrap().path();
    let actual_file_name = file_path.file_name().unwrap().to_str().unwrap();

    let orignal_hash = hash_file(file_path.to_str().unwrap());
    let default_params = localfiles::parameters::Parameters::default();
    let local_files = localfiles::Localfiles::new(&file_path.to_str().unwrap().to_string(), default_params);
    let download_link = upload(&local_files).unwrap();

    // Wait for security checks on infomaniak's side
    print!("Waiting for security checks on Infomaniak's side (30s)");
    std::thread::sleep(std::time::Duration::from_secs(30));

    download(&download_link, None, Some(TEST_FILE_DOWNLOADED_BASEPATH)).unwrap();
    let downloaded_file_path = TEST_FILE_DOWNLOADED_BASEPATH.to_string() + actual_file_name;
    let downloaded_hash = hash_file(&downloaded_file_path);
    assert_eq!(orignal_hash, downloaded_hash);
    std::fs::remove_file(&downloaded_file_path).unwrap();
}

#[test]
fn test_multiple_files_upload_download(){
    let file_paths = std::fs::read_dir(TEST_FILE_BASEPATH).unwrap().map(|entry| entry.unwrap().path()).collect::<Vec<_>>();
    let orignal_hashes = file_paths.iter().map(|path| hash_file(path.to_str().unwrap())).collect::<Vec<_>>();
    let default_params = localfiles::parameters::Parameters::default();
    let local_files = localfiles::Localfiles::new(&TEST_FILE_BASEPATH.to_string(), default_params);
    let download_link = upload(&local_files).unwrap();

    // Wait for security checks on infomaniak's side
    print!("Waiting for security checks on Infomaniak's side (30s)");
    std::thread::sleep(std::time::Duration::from_secs(30));

    download(&download_link, None, Some(TEST_FILE_DOWNLOADED_BASEPATH)).unwrap();
    let downloaded_file_paths = file_paths.iter().map(|path| TEST_FILE_DOWNLOADED_BASEPATH.to_string() + path.file_name().unwrap().to_str().unwrap()).collect::<Vec<_>>();
    let downloaded_hashes = downloaded_file_paths.iter().map(|path| hash_file(path)).collect::<Vec<_>>();
    assert_eq!(orignal_hashes, downloaded_hashes);
    downloaded_file_paths.iter().for_each(|path| std::fs::remove_file(path).unwrap());
}







