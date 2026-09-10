//! Sender email verification for uploads.
//!
//! SwissTransfer will not open a transfer for an address it has not seen confirmed.
//! It mails a six-character code to the sender, takes that code back, and hands out
//! a validation id that the transfer request then carries. The id stays good for a
//! while, so it is cached locally and only re-requested when the server rejects it.

use std::io::Write;
use std::path::PathBuf;

use serde_json::json;

use crate::api::{post, Session};
use crate::errors::SwishError;
use crate::swissfiles::SWISSTRANSFER_API;

/// Where the cache of confirmed addresses lives.
fn cache_path() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    };

    Some(base?.join("swissfer").join("email-validations.json"))
}

fn read_cache() -> serde_json::Value {
    cache_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| json!({}))
}

fn write_cache(cache: &serde_json::Value) {
    let Some(path) = cache_path() else {
        return;
    };

    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }

    // A cache miss only costs another round of verification, so failures are ignored.
    if let Ok(text) = serde_json::to_string_pretty(cache) {
        let _ = std::fs::write(path, text);
    }
}

/// The address used for the last successful upload, so `--email` can be omitted afterwards.
pub fn last_used_email() -> Option<String> {
    read_cache()["last_email"]
        .as_str()
        .map(|email| email.to_string())
}

/// A validation the server has issued for this address, and whether the code for
/// it was ever accepted. Pending ones are kept so an interrupted run can finish
/// confirming instead of asking the server for a fresh code.
struct CachedValidation {
    id: String,
    validated: bool,
}

fn cached_validation(email: &str) -> Option<CachedValidation> {
    let entry = read_cache()["validations"][email].clone();

    Some(CachedValidation {
        id: entry["id"].as_str()?.to_string(),
        validated: entry["validated"].as_bool().unwrap_or(false),
    })
}

fn remember(email: &str, validation_id: &str, validated: bool) {
    let mut cache = read_cache();

    if !cache.is_object() {
        cache = json!({});
    }
    if !cache["validations"].is_object() {
        cache["validations"] = json!({});
    }

    cache["validations"][email] = json!({ "id": validation_id, "validated": validated });
    if validated {
        cache["last_email"] = json!(email);
    }
    write_cache(&cache);
}

pub fn forget(email: &str) {
    let mut cache = read_cache();
    if let Some(validations) = cache["validations"].as_object_mut() {
        validations.remove(email);
    }
    write_cache(&cache);
}

fn api_error_code(response: &serde_json::Value) -> Option<&str> {
    if response["result"] == "error" {
        response["error"]["code"].as_str()
    } else {
        None
    }
}

fn api_error_message(response: &serde_json::Value) -> String {
    response["error"]["description"]
        .as_str()
        .unwrap_or("unknown error")
        .to_string()
}

fn call(url: &str, body: serde_json::Value, session: &Session) -> Result<serde_json::Value, SwishError> {
    let raw = post(url, body.to_string().into_bytes(), None, Some(session))?;
    let text = String::from_utf8_lossy(&raw).into_owned();

    serde_json::from_str(&text).map_err(|e| SwishError::InvalidJson {
        json: format!("{} - Response: {}", e, text),
    })
}

/// Get a validation id for `email`, asking the user for the mailed code if needed.
pub fn validation_id(email: &str, session: &Session) -> Result<String, SwishError> {
    let pending = match cached_validation(email) {
        Some(cached) if cached.validated => {
            log::debug!("Reusing confirmed validation for {}", email);
            return Ok(cached.id);
        }
        Some(cached) => {
            // A code was already sent for this address but never confirmed.
            println!("Finishing the verification already started for {}.", email);
            Some(cached.id)
        }
        None => None,
    };

    let id = match pending {
        Some(id) => id,
        None => {
            let requested = call(
                &format!("{}/email-validations", SWISSTRANSFER_API),
                json!({ "email": email }),
                session,
            )?;

            if let Some(code) = api_error_code(&requested) {
                return Err(match code {
                    "email_validation_rate_limit_error" | "email_validation_request_locked" => {
                        SwishError::InvalidResponse {
                            response: "too many verification attempts, try again later".to_string(),
                        }
                    }
                    _ => SwishError::InvalidResponse {
                        response: api_error_message(&requested),
                    },
                });
            }

            let id = requested["data"]["id"]
                .as_str()
                .ok_or_else(|| SwishError::InvalidJson {
                    json: format!("No validation id in response: {}", requested),
                })?
                .to_string();

            // Save it before prompting, so a cancelled run does not waste the code.
            remember(email, &id, false);

            println!("SwissTransfer sent a 6-character code to {}.", email);
            println!("Check your inbox, and your spam folder.");

            id
        }
    };

    for attempt in 0..3 {
        let token = prompt("Code: ")?;
        let token: String = token.chars().filter(|c| c.is_ascii_alphanumeric()).collect();

        if token.len() != 6 {
            println!("That code should be 6 letters or digits.");
            continue;
        }

        let confirmed = call(
            &format!("{}/email-validations/{}/confirm", SWISSTRANSFER_API, id),
            json!({ "token": token }),
            session,
        )?;

        match api_error_code(&confirmed) {
            None => {
                remember(email, &id, true);
                return Ok(id);
            }
            Some("invalid_validation_token") if attempt < 2 => {
                println!("That code was not accepted, try again.");
            }
            Some(_) => {
                return Err(SwishError::InvalidResponse {
                    response: api_error_message(&confirmed),
                })
            }
        }
    }

    Err(SwishError::InvalidResponse {
        response: "email verification failed".to_string(),
    })
}

fn prompt(label: &str) -> Result<String, SwishError> {
    print!("{}", label);
    std::io::stdout().flush()?;

    let mut line = String::new();
    let read = std::io::stdin().read_line(&mut line)?;

    if read == 0 {
        return Err(SwishError::InvalidResponse {
            response: "no input available to read the verification code".to_string(),
        });
    }

    Ok(line.trim().to_string())
}
