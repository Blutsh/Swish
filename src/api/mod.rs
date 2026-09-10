/// Man... told you this was a mess
use curl::easy::Easy2;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::File;
use std::sync::{Arc, Mutex};
pub mod chunks;
pub mod handlers;
use crate::errors::SwishError;
use curl::easy::List;
use handlers::DataHandler;
use handlers::DownloadHandler;
use indicatif::{ProgressBar, ProgressStyle};

/// swisstransfer.com is a normal web app now, so present as a normal browser.
/// The old `swisstransfer-webext/1.0` agent belonged to an extension that no longer exists.
pub const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36";

pub fn default_headers() -> Vec<String> {
    vec![
        format!("User-Agent: {}", USER_AGENT),
        "Accept-Language: en-GB,en;q=0.9".to_string(),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Patch,
}

/// A single HTTP response, kept whole so callers can look at the status and the
/// headers as well as the body. Unlocking a transfer needs all three.
pub struct Response {
    pub status: u32,
    pub headers: Vec<String>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// Cookie jar for one run of the tool.
///
/// Every request builds its own `Easy2`, so curl's own cookie engine has nothing to
/// hold on to between calls. Unlocking a password-protected transfer spans three
/// requests that must share the session cookie, so the jar lives here instead.
#[derive(Default)]
pub struct Session {
    cookies: RefCell<BTreeMap<String, String>>,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cookie(&self, name: &str) -> Option<String> {
        self.cookies.borrow().get(name).cloned()
    }

    pub fn cookie_header(&self) -> Option<String> {
        let jar = self.cookies.borrow();
        if jar.is_empty() {
            return None;
        }
        let pairs: Vec<String> = jar.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
        Some(format!("Cookie: {}", pairs.join("; ")))
    }

    /// Take every `Set-Cookie` from a response into the jar.
    pub fn absorb(&self, headers: &[String]) {
        for line in headers {
            let (name, value) = match line.split_once(':') {
                Some(parts) => parts,
                None => continue,
            };
            if !name.eq_ignore_ascii_case("set-cookie") {
                continue;
            }
            let pair = value.trim().split(';').next().unwrap_or_default();
            if let Some((key, val)) = pair.split_once('=') {
                self.cookies
                    .borrow_mut()
                    .insert(key.trim().to_string(), val.trim().to_string());
            }
        }
    }
}

/// Minimal percent-decoding, enough for cookie values that carry escaped padding.
pub fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&value[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Percent-encode one form field value.
pub fn form_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{:02X}", other)),
        }
    }
    out
}

fn new_easy2_data(
    url: &str,
    method: Method,
    content_type: Option<&str>,
    custom_headers: Option<Vec<String>>,
    session: Option<&Session>,
) -> Result<Easy2<DataHandler>, curl::Error> {
    let mut easy2 = Easy2::new(DataHandler::new());

    let mut merged_headers = default_headers();

    match method {
        Method::Get => {}
        Method::Post => {
            easy2.post(true)?;
            merged_headers.push("Accept: application/json".to_string());
        }
        Method::Patch => {
            easy2.custom_request("PATCH")?;
            merged_headers.push("Accept: application/json".to_string());
        }
    }

    if let Some(content_type) = content_type {
        merged_headers.push(format!("Content-Type: {}", content_type));
    }

    if let Some(session) = session {
        if let Some(cookie) = session.cookie_header() {
            merged_headers.push(cookie);
        }
    }

    if let Some(custom_headers) = custom_headers {
        merged_headers.extend(custom_headers);
    }

    let mut headers: List = List::new();
    for header in merged_headers {
        headers.append(header.as_str())?;
    }

    easy2.url(url)?;
    easy2.http_headers(headers)?;

    Ok(easy2)
}

pub fn new_easy2_download(
    url: String,
    custom_headers: Option<Vec<String>>,
    file: File,
    file_size: u64,
) -> Result<Easy2<DownloadHandler<File>>, curl::Error> {
    let progress_bar = ProgressBar::new(file_size);
    progress_bar.set_style(ProgressStyle::default_bar()
    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})").unwrap()
    .progress_chars("#>-"));

    let mut easy2 = Easy2::new(DownloadHandler {
        writer: file.try_clone().unwrap(), // Clone the file for the handler
        progress: Arc::new(Mutex::new(progress_bar)),
    });

    let mut merged_headers = default_headers();

    //add additional headers if any
    if let Some(custom_headers) = custom_headers {
        merged_headers.extend(custom_headers);
    }

    let mut headers: List = List::new();
    for header in merged_headers {
        headers.append(header.as_str())?;
    }

    easy2.url(&url)?;
    easy2.http_headers(headers)?;

    Ok(easy2)
}

/// One request, returning the whole response rather than just the body.
///
/// Redirects are deliberately not followed. Unlocking a transfer can answer with a
/// 302 to a plain `http://` URL, and following that would downgrade the connection.
pub fn request(
    url: &str,
    method: Method,
    body: Option<Vec<u8>>,
    content_type: Option<&str>,
    custom_headers: Option<Vec<String>>,
    session: Option<&Session>,
) -> Result<Response, SwishError> {
    let mut easy2 = new_easy2_data(url, method, content_type, custom_headers, session)?;

    if let Some(body) = body.as_ref() {
        easy2.post_field_size(body.len() as u64)?;
        easy2.post_fields_copy(body)?;
    }

    log::debug!("{:?} {}", method, url);
    easy2.perform()?;

    let status = easy2.response_code()?;
    let handler = easy2.get_ref();
    let response = Response {
        status,
        headers: handler.headers.clone(),
        body: handler.data.clone(),
    };

    log::debug!(
        "Response: {} - {}",
        response.status,
        response.text().chars().take(500).collect::<String>()
    );

    if let Some(session) = session {
        session.absorb(&response.headers);
    }

    Ok(response)
}

pub fn post(
    url: &str,
    body: Vec<u8>,
    additional_headers: Option<Vec<String>>,
    session: Option<&Session>,
) -> Result<Vec<u8>, SwishError> {
    let mut retries = 0;

    loop {
        let response = request(
            url,
            Method::Post,
            Some(body.clone()),
            Some("application/json"),
            additional_headers.clone(),
            session,
        )?;

        // A 4xx is the server's considered answer, so retrying only repeats it.
        // Only server-side failures are worth another go.
        if response.status < 500 || retries >= 3 {
            return Ok(response.body);
        }

        println!("Request failed, retrying... ({})", retries);
        retries += 1;
    }
}

pub fn patch(
    url: &str,
    body: Vec<u8>,
    additional_headers: Option<Vec<String>>,
    session: Option<&Session>,
) -> Result<Vec<u8>, SwishError> {
    let response = request(
        url,
        Method::Patch,
        Some(body),
        Some("application/json"),
        additional_headers,
        session,
    )?;

    if response.status >= 400 {
        return Err(SwishError::InvalidResponse {
            response: format!("{} for {}", response.status, url),
        });
    }

    Ok(response.body)
}
