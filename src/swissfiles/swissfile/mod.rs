use crate::api::chunks::{build_chunks_array, Chunk};
use crate::swissfiles::{SWISSTRANSFER_API, XSRF_COOKIE};
use crate::{
    api::{
        default_headers, new_easy2_download, patch, percent_decode, post, request, Method, Session,
    },
    errors::SwishError,
};
use indicatif::{ProgressBar, ProgressStyle};
use serde_json::json;
use std::fmt;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, PathBuf};
use std::sync::{Arc, Mutex};

const CHUNK_SIZE: usize = 52428800;

pub enum Swissfile {
    Local(LocalSwissfile),
    Remote(RemoteSwissfile),
}

pub struct LocalSwissfile {
    pub path: std::path::PathBuf,
    pub name: String,
    pub size: u64,
    pub transfer_uuid: String,
    pub files_uuid: String,
    pub chunks: Vec<Chunk>,
}

impl fmt::Display for LocalSwissfile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Name: {}, Size: {}", self.name, self.size)
    }
}

impl LocalSwissfile {
    pub fn new(path: std::path::PathBuf, transfer_uuid: &str, file_uuid: String) -> Self {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let size = path.metadata().map(|m| m.len()).unwrap_or(0);
        let chunks = build_chunks_array(size as usize, CHUNK_SIZE);

        Self {
            path,
            name,
            size,
            transfer_uuid: transfer_uuid.to_string(),
            files_uuid: file_uuid,
            chunks,
        }
    }

    /// Ask the API for a presigned storage URL to put one part into.
    ///
    /// A file that fits in a single chunk uses the file endpoint directly; anything
    /// larger is an S3-style multipart upload, one presigned URL per part.
    fn presign(&self, chunk: &Chunk, session: &Session) -> Result<String, SwishError> {
        let url = if self.is_direct_upload() {
            format!(
                "{}/transfers/{}/files/{}",
                SWISSTRANSFER_API, self.transfer_uuid, self.files_uuid
            )
        } else {
            format!(
                "{}/transfers/{}/files/{}/chunks/{}",
                SWISSTRANSFER_API,
                self.transfer_uuid,
                self.files_uuid,
                // The API numbers parts from one.
                chunk.index + 1
            )
        };

        let raw = post(&url, Vec::new(), Some(xsrf_headers(session)), Some(session))?;
        let body: serde_json::Value = serde_json::from_slice(&raw)?;

        body["data"]["url"]
            .as_str()
            .map(|url| url.to_string())
            .ok_or_else(|| SwishError::InvalidJson {
                json: format!(
                    "No upload URL in response: {}",
                    String::from_utf8_lossy(&raw)
                ),
            })
    }

    fn is_direct_upload(&self) -> bool {
        self.chunks.len() <= 1
    }

    pub fn upload(&self, session: &Session) -> Result<(), SwishError> {
        log::debug!("Uploading file: {} (UUID: {})", self.name, self.files_uuid);

        // Create a single progress bar for the entire file
        let progress_bar = ProgressBar::new(self.size);
        progress_bar.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})").unwrap()
        .progress_chars("#>-"));

        // Share this progress bar across all chunk uploads
        let progress = Arc::new(Mutex::new(progress_bar));

        let mut etags = Vec::new();

        // We want to upload each chunk in sequence
        for chunk in &self.chunks {
            log::debug!(
                "Uploading chunk {}/{} of file {}",
                chunk.index + 1,
                self.chunks.len(),
                self.name
            );

            let mut buffer = vec![0u8; chunk.size];
            if chunk.size > 0 {
                let mut file = File::open(&self.path)?;
                file.seek(SeekFrom::Start(chunk.offset as u64))?;
                file.read_exact(&mut buffer)?;
            }

            let target = self.presign(chunk, session)?;
            let etag = put_bytes(&target, &buffer, Arc::clone(&progress)).map_err(|error| {
                match error {
                    SwishError::InvalidResponse { response } => SwishError::InvalidResponse {
                        response: format!(
                            "chunk {}/{} of {}: {}",
                            chunk.index + 1,
                            self.chunks.len(),
                            self.name,
                            response
                        ),
                    },
                    other => other,
                }
            })?;

            etags.push(json!({ "chunk_index": chunk.index + 1, "etag": etag }));
        }

        // Tell the API the parts are in place so it can assemble them.
        let complete_url = format!(
            "{}/transfers/{}/files/{}",
            SWISSTRANSFER_API, self.transfer_uuid, self.files_uuid
        );
        let body = if self.is_direct_upload() {
            json!({})
        } else {
            json!({ "etags": etags })
        };

        patch(
            &complete_url,
            body.to_string().into_bytes(),
            Some(xsrf_headers(session)),
            Some(session),
        )?;

        // Make sure the progress bar is marked as finished
        progress.lock().unwrap().finish();

        Ok(())
    }
}

/// Headers the API expects on any state-changing call.
fn xsrf_headers(session: &Session) -> Vec<String> {
    let mut headers = vec!["X-Requested-With: XMLHttpRequest".to_string()];
    if let Some(token) = session.cookie(XSRF_COOKIE) {
        headers.push(format!("X-XSRF-TOKEN: {}", percent_decode(&token)));
    }
    headers
}

/// Read the ETag out of one response header line.
///
/// Storage returns it quoted, like `ETag: "abc123"`. The API checks that
/// exact form, so the quotes belong to the value and must survive. A bare
/// ETag is re-quoted here, so the caller stays correct either way.
fn parse_etag(line: &str) -> Option<String> {
    let (name, value) = line.split_once(':')?;
    if !name.eq_ignore_ascii_case("etag") {
        return None;
    }

    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    Some(if value.starts_with('"') {
        value.to_string()
    } else {
        format!("\"{}\"", value.trim_matches('"'))
    })
}

/// PUT one part straight to storage and return the ETag it answers with.
///
/// The URL is presigned, so it carries its own authorisation. Nothing from the
/// SwissTransfer session belongs on a request to a different host.
fn put_bytes(
    url: &str,
    data: &[u8],
    progress: Arc<Mutex<ProgressBar>>,
) -> Result<String, SwishError> {
    let mut easy = curl::easy::Easy::new();
    easy.url(url)?;
    easy.upload(true)?;
    easy.in_filesize(data.len() as u64)?;

    let mut list = curl::easy::List::new();
    for header in default_headers() {
        list.append(&header)?;
    }
    // S3 rejects the Expect/continue handshake curl adds for larger bodies.
    list.append("Expect:")?;
    easy.http_headers(list)?;

    let etag = Arc::new(Mutex::new(String::new()));

    {
        let mut remaining = data;
        let etag_sink = Arc::clone(&etag);

        let mut transfer = easy.transfer();
        transfer.read_function(move |into| {
            let amount = std::cmp::min(into.len(), remaining.len());
            if amount == 0 {
                return Ok(0);
            }

            into[..amount].copy_from_slice(&remaining[..amount]);
            progress.lock().unwrap().inc(amount as u64);
            remaining = &remaining[amount..];
            Ok(amount)
        })?;
        transfer.header_function(move |line| {
            if let Some(etag) = parse_etag(&String::from_utf8_lossy(line)) {
                *etag_sink.lock().unwrap() = etag;
            }
            true
        })?;
        transfer.perform()?;
    }

    let status = easy.response_code()?;
    if status >= 400 {
        return Err(SwishError::InvalidResponse {
            response: format!("storage returned {}", status),
        });
    }

    let etag = etag.lock().unwrap().clone();
    if etag.is_empty() {
        return Err(SwishError::InvalidResponse {
            response: "storage did not return an ETag for the uploaded part".to_string(),
        });
    }

    Ok(etag)
}

pub struct RemoteSwissfile {
    /// Path as the sender saw it. Carries forward slashes when the file sat in a folder.
    pub name: String,
    pub size: u64,
    pub mime_type: Option<String>,
    pub uuid: String,
    pub link_uuid: String,
}

impl fmt::Display for RemoteSwissfile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Name: {}, Size: {}, Mime: {}",
            self.name,
            self.size,
            self.mime_type.as_deref().unwrap_or("unknown")
        )
    }
}

impl RemoteSwissfile {
    pub fn new(json: &serde_json::Value, link_uuid: &str) -> Self {
        Self {
            name: json["path"].as_str().unwrap_or("unnamed").to_string(),
            size: json["size"].as_u64().unwrap_or(0),
            mime_type: json["mime_type"].as_str().map(|s| s.to_string()),
            uuid: json["id"].as_str().unwrap_or_default().to_string(),
            link_uuid: link_uuid.to_string(),
        }
    }

    /// Ask the API where this file actually lives.
    ///
    /// The answer is a presigned storage URL that expires after an hour, so it is
    /// fetched at download time rather than cached when the file list is built.
    fn resolve_url(&self, session: &Session) -> Result<String, SwishError> {
        let url = format!(
            "{}/links/{}/files/{}",
            SWISSTRANSFER_API, self.link_uuid, self.uuid
        );

        let response = request(&url, Method::Get, None, None, None, Some(session))?;

        if response.status == 403 {
            // The transfer is locked and this session never unlocked it.
            return Err(SwishError::PasswordRequired);
        }

        let body: serde_json::Value = serde_json::from_str(&response.text())?;

        if response.status != 200 || body["result"] != "success" {
            return Err(SwishError::InvalidResponse {
                response: format!("{} - {}", response.status, response.text()),
            });
        }

        body["data"]["url"]
            .as_str()
            .map(|url| url.to_string())
            .ok_or_else(|| SwishError::InvalidJson {
                json: format!("No download URL in response: {}", response.text()),
            })
    }

    /// Where this file should land, keeping any folder structure the sender used
    /// while refusing to escape the output directory.
    fn output_path(&self, custom_out_path: Option<&PathBuf>) -> PathBuf {
        let root = custom_out_path.cloned().unwrap_or_else(|| PathBuf::from("."));

        let mut path = root;
        let mut any_segment = false;

        for component in PathBuf::from(self.name.replace('\\', "/")).components() {
            if let Component::Normal(segment) = component {
                path.push(segment);
                any_segment = true;
            }
        }

        if !any_segment {
            path.push("unnamed");
        }

        path
    }

    pub fn download(
        &self,
        custom_out_path: Option<&PathBuf>,
        session: &Session,
    ) -> Result<(), SwishError> {
        let url = self.resolve_url(session)?;
        log::debug!("Downloading {} from {}", self.name, url);

        let out_path = self.output_path(custom_out_path);

        if let Some(parent) = out_path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        println!("Downloading {}", self.name);

        let file = std::fs::File::create(&out_path)?;

        let easy2 = new_easy2_download(url, None, file, self.size)?;
        easy2.perform()?;

        let status = easy2.response_code()?;
        if status >= 400 {
            // Clean up the file as it is invalid anyway
            std::fs::remove_file(&out_path)?;

            return Err(SwishError::InvalidResponse {
                response: format!("storage returned {} for {}", status, self.name),
            });
        }

        Ok(())
    }
}

impl fmt::Display for Swissfile {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Swissfile::Local(local_swissfile) => write!(f, "{}", local_swissfile),
            Swissfile::Remote(remote_swissfile) => write!(f, "{}", remote_swissfile),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_etag;

    const HEX: &str = "d8f2a1b3c4d5e6f708192a3b4c5d6e7f";

    #[test]
    fn keeps_a_quoted_etag() {
        let want = format!("\"{}\"", HEX);
        assert_eq!(parse_etag(&format!("ETag: \"{}\"\r\n", HEX)), Some(want));
    }

    #[test]
    fn requotes_a_bare_etag() {
        let want = format!("\"{}\"", HEX);
        assert_eq!(parse_etag(&format!("etag: {}\r\n", HEX)), Some(want));
    }

    #[test]
    fn ignores_other_headers() {
        assert_eq!(parse_etag("Content-Type: application/zip\r\n"), None);
    }
}
