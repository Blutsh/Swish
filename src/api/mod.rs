use curl::easy::{self, Easy2};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};
use std::vec;
pub mod chunks;
pub mod handlers;
use crate::api::chunks::build_chunks_array;
use crate::localfiles::Localfiles;
use crate::swissfiles::{swissfile, Swissfiles};
use crate::{errors::SwishError, localfiles};
use base64;
use curl::easy::List;
use handlers::DataHandler;
use handlers::DownloadHandler;
use handlers::UploadHandler;
use indicatif::{ProgressBar, ProgressStyle};
use log;
use serde_json::{de, json};

const SWISSTRANSFER_API: &str = "https://www.swisstransfer.com/api";
const DEFAULT_HEADERS: &[&str; 3] = &[
    "User-Agent: swisstransfer-webext/1.0",
    "Cookie: webext=1",
    "Referer: swish/0.1",
];

fn new_easy2_data(
    url: String,
    custom_headers: Option<Vec<String>>,
    post: bool,
) -> Result<Easy2<DataHandler>, curl::Error> {
    let mut easy2 = Easy2::new(DataHandler { data: Vec::new() });

    let mut merged_headers: Vec<String> = DEFAULT_HEADERS.iter().map(|x| x.to_string()).collect();

    if post {
        easy2.post(true)?;

        //add headers
        merged_headers.push("Content-Type: application/json".to_string());
        merged_headers.push("Accept: application/json".to_string());
    }

    //add additional headers if any
    if let Some(custom_headers) = custom_headers {
        for header in custom_headers {
            merged_headers.push(header);
        }
    }

    let mut headers: List = List::new();
    for header in merged_headers {
        headers.append(header.as_str())?;
    }

    easy2.url(&url)?;
    easy2.http_headers(headers)?;

    Ok(easy2)
}

fn new_easy2_download(
    url: String,
    custom_headers: Option<Vec<String>>,
    file: File,
) -> Result<Easy2<DownloadHandler<File>>, curl::Error> {
    let file_metadata = file.metadata().unwrap();
    let file_size = file_metadata.len();

    let mut easy2 = Easy2::new(DownloadHandler {
        writer: file.try_clone().unwrap(), // Clone the file for the handler
        progress: Arc::new(Mutex::new(ProgressBar::new(file_size as u64))),
    });

    let mut merged_headers: Vec<String> = DEFAULT_HEADERS.iter().map(|x| x.to_string()).collect();

    //add additional headers if any
    if let Some(custom_headers) = custom_headers {
        for header in custom_headers {
            merged_headers.push(header);
        }
    }

    let mut headers: List = List::new();
    for header in merged_headers {
        headers.append(header.as_str())?;
    }

    easy2.url(&url)?;
    easy2.http_headers(headers)?;

    Ok(easy2)
}

fn new_easy2_upload(
    url: String,
    custom_headers: Option<Vec<String>>,
    reader: &[u8], 
) -> Result<Easy2<UploadHandler<&[u8]>>, curl::Error> {
    let mut easy2 = Easy2::new(UploadHandler {
        reader,
        progress: Arc::new(Mutex::new(ProgressBar::new(reader.len() as u64))), // Set the progress bar length based on reader length
    });

    let mut merged_headers: Vec<String> = DEFAULT_HEADERS.iter().map(|x| x.to_string()).collect();

    //add headers
    merged_headers.push("Content-Type: application/json".to_string());
    merged_headers.push("Accept: application/json".to_string());

    //add additional headers if any
    if let Some(custom_headers) = custom_headers {
        for header in custom_headers {
            merged_headers.push(header);
        }
    }

    let mut headers: List = List::new();
    for header in merged_headers {
        headers.append(header.as_str())?;
    }

    easy2.url(&url)?;
    easy2.http_headers(headers)?;
    easy2.post(true)?;
    easy2.upload(true)?;
    easy2.http_version(curl::easy::HttpVersion::V11)?;


    Ok(easy2)
}


fn get(url: &str, additional_headers: Option<Vec<String>>) -> Result<String, SwishError> {
    let mut easy2 = new_easy2_data(url.to_string(), additional_headers, false)?;

    log::debug!("Sending get request to: {}", url);

    easy2.perform()?;

    log::debug!(
        "Response: {} - {:?}",
        easy2.response_code()?,
        String::from_utf8(easy2.get_ref().data.clone()).unwrap()
    );

    match easy2.response_code()? {
        200 => {
            let data = easy2.get_ref().data.clone();
            Ok(String::from_utf8(data).unwrap())
        }

        404 => Err(SwishError::NotFound {
            url: url.to_string(),
        }),
        code => Err(SwishError::InvalidResponse {
            response: code.to_string(),
        }),
    }
}

fn post(
    url: &str,
    body: Vec<u8>,
    additional_headers: Option<Vec<String>>,
) -> Result<Vec<u8>, SwishError> {
    log::debug!("Sending post request to: {}", url);
    log::debug!("Body: {}", String::from_utf8(body.clone()).unwrap());
    let mut retries = 0;

    let mut easy2 = new_easy2_data(url.to_string(), additional_headers, true)?;
    easy2.post_fields_copy(&body)?;

    loop {
        easy2.perform()?;
        log::debug!(
            "Response: {} - {:?}",
            easy2.response_code()?,
            String::from_utf8(easy2.get_ref().data.clone()).unwrap()
        );

        if easy2.response_code()? < 400 || retries >= 3 {
            let data = easy2.get_ref().data.clone();
            return Ok(data);
        } else {
            println!("Request failed, retrying... ({})", retries);
            retries += 1;
        }
    }
}

pub fn download(swisstransfer_link: &str, password: Option<&str>) -> Result<(), SwishError> {
    let download_id = swisstransfer_link.split("/").last().unwrap();
    let url = format!(
        "{}/links/{}",
        "https://www.swisstransfer.com/api", download_id
    );

    let auth_header: Option<Vec<String>> = match password {
        Some(password) => {
            log::debug!("Password has been provided. Adding authorization header.");

            //encode password to base64
            let password = base64::encode(password);

            //add authorization header
            let header = "Authorization: ".to_string() + &password;
            Some(vec![header])
        }
        None => None,
    };

    let response = get(&url, auth_header)?;
    let response: serde_json::Value = serde_json::from_str(&response).unwrap();

    let message = response["data"]["message"].as_str();

    match message {
        Some("Transfer need a password") => return Err(SwishError::PasswordRequired),
        Some("The password is wrong") => return Err(SwishError::InvalidPassword),
        _ => (),
    }

    log::debug!("Response after authorization: {:?}", response);

    let swissfiles = Swissfiles::new(&response);

    for swissfile in swissfiles.files {
        let mut file = std::fs::File::create(&swissfile.name)?;

        let mut easy2 = new_easy2_download(
            build_download_url(
                &swissfile,
                swissfiles.need_password,
                &swissfiles.container_uuid,
                password,
            ),
            None,
            file,
        )?;

        easy2.perform()?;
    }
    Ok(())
}

fn build_download_url(
    swissfile: &swissfile::Swissfile,
    need_password: bool,
    container_uuid: &str,
    password: Option<&str>,
) -> String {
    let mut url = swissfile.url.clone();

    if need_password {
        let token = generate_download_token(password.unwrap(), container_uuid).unwrap();
        let token: String =
            serde_json::from_str(token.as_str()).unwrap_or_else(|_| token.to_string());
        url = format!("{}?token={}", swissfile.url, token);
    }

    url
}

pub fn upload( files: &localfiles::Localfiles) -> Result<(), Box<dyn std::error::Error>> {
    let total_size: usize = files.files.iter().map(|file| file.size as usize).sum();
    let container = get_container(files).unwrap();

    let upload_host = container["uploadHost"].as_str().unwrap();
    let container_uuid = container["container"]["UUID"].as_str().unwrap();
    let files_uuid = container["filesUUID"][0].as_str().unwrap();
    let progress = Arc::new(Mutex::new(ProgressBar::new(total_size as u64)));
    progress.lock().unwrap().set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})").unwrap()
            .progress_chars("#>-"));

    for file in files.files.iter() {
        let chunks = build_chunks_array(file.size as usize, 52428800);
        let full_path = if files.path.ends_with(&file.name) {
            files.path.clone()
        } else {
            format!("{}/{}", files.path, file.name)
        };
        let mut file = std::fs::File::open(&full_path)?;

        // Iterate over a reference to chunks to avoid moving it
        for chunk in &chunks {
            file.seek(SeekFrom::Start(chunk.offset as u64))?;
            let mut buffer = vec![0; chunk.size];
            file.read_exact(&mut buffer)?;

            let upload_url = format!(
                "https://{}/api/uploadChunk/{}/{}/{}/{}",
                upload_host,
                container_uuid,
                files_uuid,
                chunk.index,
                if chunk.index == chunks.len() - 1 {
                    "1"
                } else {
                    "0"
                }
            );
           let mut easy2 = new_easy2_upload(upload_url, None, &buffer)?;

            easy2.post(true)?;
            easy2.post_field_size(chunk.size as u64)?;
            easy2.perform()?;
        }
    }

    // Send a final request to the uploadComplete endpoint
    let url = format!("{}/uploadComplete", "https://www.swisstransfer.com/api");
    let body = json!({
        "UUID": container_uuid,
        "lang": "en_GB"
    })
    .to_string()
    .into_bytes();
    let response = post(&url, body, None)?;
    progress.lock().unwrap().finish();

    create_download_link(&response)?;

    Ok(())
}

fn get_container(localfiles: &Localfiles) -> Result<serde_json::Value, SwishError> {
    let url = format!("{}/containers", "https://www.swisstransfer.com/api");

    let files: Vec<_> = localfiles
        .files
        .iter()
        .map(|file| {
            json!({
                "name": file.name,
                "size": file.size
            })
        })
        .collect();

    let files_string = serde_json::to_string(&files).unwrap();

    let payload = json!({
    "duration": localfiles.parameters.duration,
    "authorEmail": localfiles.parameters.author_email,
    "password": localfiles.parameters.password,
    "message": localfiles.parameters.message,
    "sizeOfUpload": localfiles.files.iter().map(|file| file.size).sum::<u64>(),
    "numberOfDownload": localfiles.parameters.number_of_download,
    "numberOfFile": localfiles.files.len(),
    "lang": localfiles.parameters.lang,
    "recaptcha": "nope",
    "files": files_string,
    "recipientsEmails": "[]"
        });

    let payload_string = serde_json::to_string(&payload).unwrap();
    let payload_bytes = payload_string.as_bytes();

    let response = post(url.as_str(), payload_bytes.to_vec(), None)?;

    Ok(serde_json::from_str(&String::from_utf8(response).unwrap()).unwrap())
}

fn create_download_link(response: &Vec<u8>) -> Result<String, SwishError> {
    //convert u8 to json object
    let response: serde_json::Value =
        serde_json::from_str(&String::from_utf8(response.to_vec()).unwrap()).unwrap();

    let link = format!(
        "https://www.swisstransfer.com/d/{}",
        response[0]["linkUUID"].as_str().unwrap()
    );

    println!("Download link: {}", link);
    Ok(link)
}
fn generate_download_token(password: &str, container_uuid: &str) -> Result<String, SwishError> {
    //At this point we already sent the password trough the headers
    // but we need to gen a token by posting to /api/generateDownloadToken
    // with the following payload:
    //{"password":"123123","containerUUID":"c8deff85-e7ad-4df7-8c45-0f48542af26b","fileUUID":"f3d1b103-6d10-404c-b636-826cf6fe788e"}
    let url = format!(
        "{}/generateDownloadToken",
        "https://www.swisstransfer.com/api"
    );
    let payload = json!({
        "password": password,
        "containerUUID": container_uuid,
        "fileUUID": serde_json::Value::Null
    });

    let response = post(url.as_str(), payload.to_string().into_bytes(), None)?;
    let response: String = String::from_utf8(response).unwrap();

    log::debug!("Response: {:?}", response);

    return Ok(response);

    //here we got the token
    // then we need to get the file with GET /api/download/ea718c12-3b1c-40a9-a8a2-69510e1f1290/f3d1b103-6d10-404c-b636-826cf6fe788e?token=80f11de4-5bb5-43b1-999e-ce46cac6600e
}

//test
