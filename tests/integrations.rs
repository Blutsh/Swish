use swish::swissfiles::{uploadparameters::UploadParameters, Swissfiles};


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
    let default_params = UploadParameters::default();

    //upload the file
    let local_files = Swissfiles::new_localfiles(file_path.clone(), &default_params).unwrap();
    let download_link = local_files.upload().unwrap();

    // Download the file
    let remote_files = Swissfiles::new_remotefiles(&download_link, None).unwrap();
    remote_files.download(None).unwrap();

    let downloaded_file_path = TEST_FILE_DOWNLOADED_BASEPATH.to_string() + actual_file_name;
    let downloaded_hash = hash_file(&downloaded_file_path);
    assert_eq!(orignal_hash, downloaded_hash);
    std::fs::remove_file(&downloaded_file_path).unwrap();
}







