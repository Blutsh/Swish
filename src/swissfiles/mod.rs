use std::io::Write;
use std::sync::OnceLock;
use std::{fmt, path::PathBuf};

mod swissfile;
pub mod uploadparameters;
use crate::{
    api::{form_encode, patch, percent_decode, post, request, Method, Session},
    email,
    errors::SwishError,
    swissfiles::swissfile::{RemoteSwissfile, Swissfile},
};
use regex::Regex;
use serde_json::json;

use self::uploadparameters::UploadParameters;

pub const SWISSTRANSFER_BASE: &str = "https://www.swisstransfer.com";
pub const SWISSTRANSFER_API: &str = "https://www.swisstransfer.com/api/1";

/// Cookie the site uses for its CSRF token. Laravel default name, renamed by Infomaniak.
pub(crate) const XSRF_COOKIE: &str = "SWISSTRANSFER-API-XSRF-TOKEN";

/// How long to wait between antivirus polls.
const ANTIVIRUS_POLL: std::time::Duration = std::time::Duration::from_secs(5);

/// swisstransfer.com is an Inertia.js app: every page ships its whole state as JSON
/// in a single script tag. Reading that is how we learn what a transfer contains,
/// and it replaces the `/api/links/{uuid}` endpoint that was retired.
struct InertiaPage {
    component: String,
    version: String,
    props: serde_json::Value,
}

impl InertiaPage {
    fn from_html(html: &str) -> Result<Self, SwishError> {
        static PAGE_RE: OnceLock<Regex> = OnceLock::new();
        let re = PAGE_RE.get_or_init(|| {
            Regex::new(
                r#"(?s)<script data-page="app" type="application/json">(.*?)</script>"#,
            )
            .unwrap()
        });

        let captures = re.captures(html).ok_or_else(|| SwishError::InvalidResponse {
            response: "no Inertia page data found, SwissTransfer may have changed again"
                .to_string(),
        })?;

        Self::from_json(&html_unescape(&captures[1]))
    }

    fn from_json(json: &str) -> Result<Self, SwishError> {
        let value: serde_json::Value = serde_json::from_str(json)?;

        Ok(Self {
            component: value["component"].as_str().unwrap_or_default().to_string(),
            version: value["version"].as_str().unwrap_or_default().to_string(),
            props: value["props"].clone(),
        })
    }
}

/// The embedded JSON is normally emitted raw, but unescape the few entities that
/// would appear if a transfer title ever contained markup.
fn html_unescape(input: &str) -> String {
    if !input.contains('&') {
        return input.to_string();
    }

    input
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn link_url(link_uuid: &str) -> String {
    format!("{}/dl/{}", SWISSTRANSFER_BASE, link_uuid)
}

fn fetch_page(link_uuid: &str, session: &Session) -> Result<InertiaPage, SwishError> {
    let url = link_url(link_uuid);
    let response = request(&url, Method::Get, None, None, None, Some(session))?;

    match response.status {
        200 => InertiaPage::from_html(&response.text()),
        404 => Err(SwishError::NotFound { url }),
        code => Err(SwishError::InvalidResponse {
            response: format!("{} for {}", code, url),
        }),
    }
}

/// Unlock a password-protected transfer.
///
/// The old base64 `Authorization` header is gone. The site now posts the password
/// back to the link itself and remembers the result in a Laravel session, so the
/// cookies collected while fetching the page have to ride along, together with the
/// CSRF token the site handed out.
fn unlock(
    link_uuid: &str,
    password: &str,
    version: &str,
    session: &Session,
) -> Result<InertiaPage, SwishError> {
    let url = link_url(link_uuid);

    let xsrf = session
        .cookie(XSRF_COOKIE)
        .map(|token| percent_decode(&token))
        .ok_or_else(|| SwishError::InvalidResponse {
            response: format!("SwissTransfer did not issue a {} cookie", XSRF_COOKIE),
        })?;

    let headers = vec![
        "X-Inertia: true".to_string(),
        format!("X-Inertia-Version: {}", version),
        format!("X-XSRF-TOKEN: {}", xsrf),
        "Accept: text/html, application/xhtml+xml".to_string(),
    ];

    let body = format!("password={}", form_encode(password)).into_bytes();

    let response = request(
        &url,
        Method::Post,
        Some(body),
        Some("application/x-www-form-urlencoded"),
        Some(headers),
        Some(session),
    )?;

    // With the Inertia header set, a successful unlock answers with the next page as
    // JSON. A redirect means the answer is only in the session, so ask for it again.
    let page = if response.status == 200 && response.body.starts_with(b"{") {
        InertiaPage::from_json(&response.text())?
    } else {
        fetch_page(link_uuid, session)?
    };

    if page.component == "link/password" {
        return Err(SwishError::InvalidPassword);
    }

    Ok(page)
}

pub struct Swissfiles {
    pub files: Vec<Swissfile>,
    pub transfer_uuid: String,
    session: Session,
}

impl Swissfiles {
    /// Build the file list for a download link.
    ///
    /// `link_uuid` is the identifier out of the share URL, already validated by the
    /// caller. Three states can come back before the files do: a password prompt, an
    /// antivirus scan still running, or one of several dead ends.
    pub fn new_remotefiles(link_uuid: &str, password: Option<&str>) -> Result<Self, SwishError> {
        log::debug!("Reading transfer {}", link_uuid);

        let session = Session::new();
        let mut page = fetch_page(link_uuid, &session)?;

        if page.component == "link/password" {
            let password = password.ok_or(SwishError::PasswordRequired)?;
            page = unlock(link_uuid, password, &page.version, &session)?;
        }

        while page.component == "errors/transfer/waiting-for-antivirus" {
            println!("Waiting for security checks on Infomaniak's side...");
            std::io::stdout().flush().ok();
            std::thread::sleep(ANTIVIRUS_POLL);
            page = fetch_page(link_uuid, &session)?;
        }

        match page.component.as_str() {
            "link/show" => (),
            "errors/transfer/not-found" => {
                return Err(SwishError::NotFound {
                    url: link_url(link_uuid),
                })
            }
            "errors/transfer/unavailable" => return Err(SwishError::DownloadNumberExceeded),
            "errors/transfer/infected" => return Err(SwishError::TransferInfected),
            "errors/transfer/country-blocked" => return Err(SwishError::CountryBlocked),
            other => {
                return Err(SwishError::UnexpectedState {
                    state: other.to_string(),
                })
            }
        }

        let transfer = &page.props["transfer"];
        let transfer_uuid = transfer["id"].as_str().unwrap_or_default().to_string();

        let file_list = transfer["files"]
            .as_array()
            .ok_or_else(|| SwishError::InvalidJson {
                json: "transfer contains no file list".to_string(),
            })?;

        let files = file_list
            .iter()
            .map(|file| Swissfile::Remote(RemoteSwissfile::new(file, link_uuid)))
            .collect();

        Ok(Swissfiles {
            files,
            transfer_uuid,
            session,
        })
    }

    pub fn new_localfiles(
        path: PathBuf,
        upload_parameter: &UploadParameters,
    ) -> Result<Self, SwishError> {
        // Get all files in the directory or just the single file
        let files = if path.is_dir() {
            std::fs::read_dir(path)
                .map_err(|e| SwishError::FileError { error: e })?
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter(|p| p.is_file()) // Only include files, not subdirectories
                .collect::<Vec<_>>()
        } else {
            vec![path]
        };

        // Early return if no files found
        if files.is_empty() {
            return Err(SwishError::FileError {
                error: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "No files found in directory",
                ),
            });
        }

        let session = Session::new();

        // Creating a transfer is CSRF-protected, so a page has to be loaded first to
        // pick up the session and token cookies.
        let home = request(
            &format!("{}/en", SWISSTRANSFER_BASE),
            Method::Get,
            None,
            None,
            None,
            Some(&session),
        )?;
        let version = InertiaPage::from_html(&home.text())
            .map(|page| page.version)
            .unwrap_or_default();

        let email = upload_parameter
            .author_email
            .clone()
            .or_else(email::last_used_email)
            .ok_or(SwishError::EmailRequired)?;

        // SwissTransfer only opens transfers for a confirmed sender address.
        let mut validation_id = email::validation_id(&email, &session)?;

        let transfer = match create_transfer(
            &files,
            upload_parameter,
            &email,
            &validation_id,
            &version,
            &session,
        ) {
            Err(SwishError::EmailValidationExpired) => {
                // A cached id can go stale; drop it and confirm the address again.
                email::forget(&email);
                validation_id = email::validation_id(&email, &session)?;
                create_transfer(
                    &files,
                    upload_parameter,
                    &email,
                    &validation_id,
                    &version,
                    &session,
                )?
            }
            other => other?,
        };

        log::debug!(
            "Transfer response: {}",
            serde_json::to_string(&transfer).unwrap_or_default()
        );

        let transfer_uuid = transfer["id"].as_str().unwrap_or_default().to_string();

        let remote_files = transfer["files"]
            .as_array()
            .ok_or_else(|| SwishError::InvalidJson {
                json: "transfer response carries no file list".to_string(),
            })?;

        let mut swissfiles = Vec::new();

        for (index, file_path) in files.iter().enumerate() {
            let name = file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            // The server keys its acknowledgement by path, not by position.
            let remote = remote_files
                .iter()
                .find(|remote| remote["path"] == name)
                .or_else(|| remote_files.get(index));

            let Some(remote) = remote else {
                log::warn!("Server did not acknowledge {}", name);
                continue;
            };

            let file_uuid = remote["id"].as_str().unwrap_or_default().to_string();
            swissfiles.push(Swissfile::Local(swissfile::LocalSwissfile::new(
                file_path.clone(),
                &transfer_uuid,
                file_uuid,
            )));
        }

        if swissfiles.is_empty() {
            return Err(SwishError::InvalidResponse {
                response: "the server acknowledged none of the files".to_string(),
            });
        }

        Ok(Swissfiles {
            files: swissfiles,
            transfer_uuid,
            session,
        })
    }

    pub fn download(&self, custom_out_path: Option<&PathBuf>) -> Result<(), SwishError> {
        // Create the directory if it doesn't exist or use the current directory
        let out_path = match custom_out_path {
            Some(path) => {
                if !path.exists() {
                    std::fs::create_dir_all(path)?;
                }
                Some(path)
            }
            None => None,
        };

        for file in &self.files {
            match file {
                Swissfile::Local(_) => {
                    // Handle local file download
                    unimplemented!("Humm, Why would you want to download a local file ?")
                }
                Swissfile::Remote(remote_swissfile) => {
                    // Call download method on RemoteSwissfile
                    remote_swissfile.download(out_path, &self.session)?;
                }
            }
        }
        Ok(())
    }

    pub fn upload(&self) -> Result<String, SwishError> {
        for file in &self.files {
            match file {
                Swissfile::Local(local_swissfile) => {
                    // Call upload method on LocalSwissfile
                    local_swissfile.upload(&self.session)?;
                }
                Swissfile::Remote(_) => {
                    // Handle remote file upload
                    unimplemented!("Humm, Why would you want to upload a remote file ?")
                }
            }
        }

        let download_link = self.finalize_upload()?;
        Ok(download_link)
    }

    /// Mark the transfer complete, which is what makes the share link exist.
    fn finalize_upload(&self) -> Result<String, SwishError> {
        let url = format!("{}/transfers/{}", SWISSTRANSFER_API, self.transfer_uuid);
        let body = json!({ "status": "completed" }).to_string().into_bytes();

        let mut headers = vec!["X-Requested-With: XMLHttpRequest".to_string()];
        if let Some(token) = self.session.cookie(XSRF_COOKIE) {
            headers.push(format!("X-XSRF-TOKEN: {}", percent_decode(&token)));
        }

        let response = patch(&url, body, Some(headers), Some(&self.session))?;

        create_download_link(&response)
    }
}

impl fmt::Display for Swissfiles {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for file in &self.files {
            writeln!(f, "{}", file)?;
        }
        Ok(())
    }
}

/// Ask SwissTransfer to open a transfer and hand back an id per file.
///
/// The service protects this endpoint with Altcha, a proof-of-work and behavioural
/// anti-bot challenge. This tool does not solve it: the field is sent empty and a
/// rejection is reported as such.
fn create_transfer(
    files: &[PathBuf],
    upload_parameter: &UploadParameters,
    email: &str,
    validation_id: &str,
    version: &str,
    session: &Session,
) -> Result<serde_json::Value, SwishError> {
    let url = format!("{}/transfers", SWISSTRANSFER_API);

    let files_json: Vec<_> = files
        .iter()
        .filter(|file| file.is_file())
        .map(|file| {
            let name = file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
            let size = file.metadata().map(|m| m.len()).unwrap_or(0);

            json!({
                "path": name,
                "size": size,
                "mime_type": serde_json::Value::Null
            })
        })
        .collect();

    // Early return if no valid files
    if files_json.is_empty() {
        return Err(SwishError::FileError {
            error: std::io::Error::new(std::io::ErrorKind::NotFound, "No valid files to upload"),
        });
    }

    let payload = json!({
        "method": "link",
        "title": "",
        "recipients": upload_parameter.recipients_emails,
        "email": email,
        "message": upload_parameter.message,
        "password": upload_parameter.password,
        "language": upload_parameter.lang,
        "max_download": upload_parameter.number_of_download,
        "expires_in_days": upload_parameter.duration,
        "files": files_json,
        "altcha": "",
        "email_validation_id": validation_id
    });

    let mut headers = vec!["X-Requested-With: XMLHttpRequest".to_string()];
    if !version.is_empty() {
        headers.push(format!("X-Inertia-Version: {}", version));
    }
    if let Some(token) = session.cookie(XSRF_COOKIE) {
        headers.push(format!("X-XSRF-TOKEN: {}", percent_decode(&token)));
    }

    let response = post(
        url.as_str(),
        payload.to_string().into_bytes(),
        Some(headers),
        Some(session),
    )?;

    let response_str = String::from_utf8_lossy(&response).into_owned();

    let response_json: serde_json::Value =
        serde_json::from_str(&response_str).map_err(|e| SwishError::InvalidJson {
            json: format!(
                "Failed to parse transfer JSON: {} - Response: {}",
                e, response_str
            ),
        })?;

    if response_json["result"] == "error" {
        if rejected_field(&response_json, "altcha") {
            return Err(SwishError::UploadBlocked);
        }
        if response_str.contains("verified_email_check_failed") {
            return Err(SwishError::EmailValidationExpired);
        }

        return Err(SwishError::InvalidResponse {
            response: describe_validation_errors(&response_json)
                .unwrap_or_else(|| response_str.clone()),
        });
    }

    // The payload sits under `data` when the API wraps it, and at the root otherwise.
    if response_json["data"].is_object() {
        Ok(response_json["data"].clone())
    } else {
        Ok(response_json)
    }
}

/// Whether the server complained about one named field.
fn rejected_field(response: &serde_json::Value, field: &str) -> bool {
    response["error"]["errors"]
        .as_array()
        .map(|errors| {
            errors
                .iter()
                .any(|error| error["context"]["attribute"] == field)
        })
        .unwrap_or(false)
}

/// Flatten the API's per-field validation errors into one readable line.
fn describe_validation_errors(response: &serde_json::Value) -> Option<String> {
    let errors = response["error"]["errors"].as_array()?;

    let described: Vec<String> = errors
        .iter()
        .map(|error| {
            let field = error["context"]["attribute"].as_str().unwrap_or("request");
            let message = error["description"].as_str().unwrap_or("rejected");
            format!("{}: {}", field, message)
        })
        .collect();

    (!described.is_empty()).then(|| described.join(", "))
}

pub fn create_download_link(response: &[u8]) -> Result<String, SwishError> {
    let response_str = String::from_utf8_lossy(response).into_owned();

    log::debug!("Download link response: {}", response_str);

    let response_json: serde_json::Value =
        serde_json::from_str(&response_str).map_err(|e| SwishError::InvalidJson {
            json: format!("Failed to parse JSON: {} - Response: {}", e, response_str),
        })?;

    // The completed transfer carries the finished share URL.
    if let Some(url) = response_json["data"]["link"]["download_url"].as_str() {
        return Ok(url.to_string());
    }

    // Fall back to rebuilding it from the link id.
    let link_uuid = [
        &response_json["data"]["link"]["id"],
        &response_json["link"]["id"],
    ]
    .into_iter()
    .find_map(|value| value.as_str())
    .ok_or_else(|| SwishError::InvalidJson {
        json: format!("Missing download link in response: {}", response_str),
    })?;

    Ok(format!("{}/dl/{}", SWISSTRANSFER_BASE, link_uuid))
}
