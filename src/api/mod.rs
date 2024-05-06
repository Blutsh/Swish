use curl::easy::Easy2;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::vec;
pub mod chunks;
pub mod handlers;
use crate::api::chunks::build_chunks_array;
use crate::localfiles::Localfiles;
use crate::swissfiles::{Swissfiles};
use crate::{errors::SwishError, localfiles};
use base64;
use curl::easy::List;
use handlers::DataHandler;
use handlers::DownloadHandler;
use handlers::UploadHandler;
use indicatif::{ProgressBar, ProgressStyle};
use log;
use serde_json::json;

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

pub fn new_easy2_download(
    url: String,
    custom_headers: Option<Vec<String>>,
    file: File,
) -> Result<Easy2<DownloadHandler<File>>, curl::Error> {
    let file_metadata = file.metadata().unwrap();
    let file_size = file_metadata.len();

    let progress_bar = ProgressBar::new(file_size as u64);
    progress_bar.set_style(ProgressStyle::default_bar()
    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})").unwrap()
    .progress_chars("#>-"));

    let mut easy2 = Easy2::new(DownloadHandler {
        writer: file.try_clone().unwrap(), // Clone the file for the handler
        progress: Arc::new(Mutex::new(progress_bar)),
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
    upload_total_size: usize,
    reader: &File,
) -> Result<Easy2<UploadHandler<&File>>, curl::Error> {
    let progress_bar = ProgressBar::new(upload_total_size as u64);
    progress_bar.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})").unwrap()
        .progress_chars("#>-"));

    let mut easy2 = Easy2::new(UploadHandler {
        reader,
        progress: Arc::new(Mutex::new(progress_bar)),
    });

    let mut merged_headers: Vec<String> = DEFAULT_HEADERS.iter().map(|x| x.to_string()).collect();

    // add headers
    merged_headers.push("Content-Type: application/json".to_string());
    merged_headers.push("Accept: application/json".to_string());

    // add additional headers if any
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

pub fn get(url: &str, additional_headers: Option<Vec<String>>) -> Result<String, SwishError> {
    let additional_headers2 = additional_headers.clone();
    let mut easy2 = new_easy2_data(url.to_string(), additional_headers, false)?;

    log::debug!("Sending get request to: {} \n with headers {}", url, additional_headers2.unwrap_or_default().join("\n"));

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

pub fn post(
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


pub fn upload(files: &localfiles::Localfiles) -> Result<String, SwishError> {
    let total_size: usize = files.files.iter().map(|file| file.size as usize).sum();
    let container = get_container(files).unwrap();
    let upload_host = container["uploadHost"].as_str().unwrap();
    let container_uuid = container["container"]["UUID"].as_str().unwrap();
    let files_uuid = container["filesUUID"][0].as_str().unwrap();

    for file in &files.files {
        let chunks = build_chunks_array(file.size as usize, 52428800);
        let full_path = if files.path.ends_with(&file.name) {
            files.path.clone()
        } else {
            format!("{}/{}", files.path, file.name)
        };
        let file = File::open(&full_path)?; // Make file mutable
        let mut easy2 = new_easy2_upload("".to_string(), None, total_size, &file)?; // Pass a reference to file

        // Iterate over a reference to chunks to avoid moving it
        for chunk in &chunks {
            let mut file = File::open(&full_path)?; // Make file mutable inside the loop
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
            easy2.url(&upload_url)?;
            easy2.post(true)?;
            easy2.post_field_size(chunk.size as u64)?;
            easy2.perform()?;
        }
    }

    // Send a final request to the uploadComplete endpoint
    let url = format!("{}/uploadComplete", SWISSTRANSFER_API);
    let body = json!({
        "UUID": container_uuid,
        "lang": "en_GB"
    })
    .to_string()
    .into_bytes();
    let response = post(&url, body, None)?;

    Ok(create_download_link(&response)?)
}

fn get_container(localfiles: &Localfiles) -> Result<serde_json::Value, SwishError> {
    let url = format!("{}/containers", SWISSTRANSFER_API);

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

    Ok(link)
}

