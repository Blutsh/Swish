mod api;
mod errors;
mod swissfiles;
use std::path::PathBuf;

use clap::Parser;
use errors::SwishError;
use regex::Regex;

#[derive(clap::Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// could be a file or a folder or a link
    file: String,

    /// Sets the password for the file(s) downloaded / uploaded
    #[arg(short, long, value_name = "password")]
    password: Option<String>,

    /// Define the email recipients for the file(s) uploaded
    #[arg(short, long, value_name = "john@doe.com, ...", value_parser = validate_email)]
    recipients_email: Option<String>,

    /// Define the email author for the file(s) uploaded
    #[arg(short, long, value_name = "john@doe.com", value_parser = validate_email)]
    author_email: Option<String>,

    /// Define the message for the file(s) uploaded
    #[arg(short, long, value_name = "Hello World")]
    message: Option<String>,

    /// Define the max number of downloads for the file(s) uploaded
    #[arg(short, long, value_name = "250", value_parser = validate_number_download)]
    number_download: Option<PathBuf>,

    /// Define the number of days the file(s) will be available for download
    #[arg(short, long, value_name = "30", value_parser = validate_duration)]
    duration: Option<String>,
}

fn main() -> Result<(), SwishError> {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .init()
        .unwrap();
    let cli = Cli::parse();
    let arg = cli.file;

    //check if the arg is a link
    if is_swisstransfer_link(&arg) {
        //Construct the swissfiles from the link
        let swissfiles = swissfiles::Swissfiles::new_remotefiles(&arg, cli.password.as_deref())?;

        //Download the files
        swissfiles.download(None)?;

        return Ok(());
    }

    if path_exists(&arg) {
        let mut params = swissfiles::uploadparameters::UploadParameters::default();

        if let Some(password) = cli.password {
            params.password = password;
        }

        if let Some(recipients_email) = cli.recipients_email {
            params.recipients_emails = recipients_email
                .split(",")
                .map(|email| email.to_string())
                .collect();
        }

        if let Some(author_email) = cli.author_email {
            params.author_email = author_email;
        }

        if let Some(message) = cli.message {
            params.message = message;
        }

        if let Some(number_download) = cli.number_download {
            params.number_of_download = number_download.to_str().unwrap().parse().unwrap();
        }

        if let Some(duration) = cli.duration {
            params.duration = duration.parse().unwrap();
        }

        let local_files = swissfiles::Swissfiles::new_localfiles(&arg, &params)?;
        local_files.upload()?;

        return Ok(());
    }

    Err(SwishError::InvalidUrl { url: arg })
}

fn verify_link_format(link: &str) -> bool {
    let re = Regex::new(r"^https://www\.swisstransfer\.com/d/[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$").unwrap();
    re.is_match(link)
}

fn is_swisstransfer_link(link: &str) -> bool {
    let re = Regex::new(r"^https://www\.swisstransfer\.com/d/[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$").unwrap();
    re.is_match(link)
}

fn path_exists(path: &str) -> bool {
    //str is a file or folder
    PathBuf::from(path).exists()
}

fn validate_email(val: &str) -> Result<(), String> {
    let re = Regex::new(r"^[a-zA-Z0-9_.+-]+@[a-zA-Z0-9-]+\.[a-zA-Z0-9-.]+$").unwrap();
    val.split(',').map(str::trim).try_for_each(|email| {
        if re.is_match(email) {
            Ok(())
        } else {
            Err(format!("\"{}\" is not a valid email", email))
        }
    })
}

fn validate_number_download(val: &str) -> Result<(), String> {
    let number = val.parse::<u32>().map_err(|_| "Must be a valid number")?;
    if number < 1 || number > 250 {
        Err(String::from(
            "Number of downloads must be between 1 and 250",
        ))
    } else {
        Ok(())
    }
}

fn validate_duration(val: &str) -> Result<(), String> {
    let number = val.parse::<u32>().map_err(|_| "Must be a valid number")?;
    if number < 1 || number > 30 {
        Err(String::from("Duration must be between 1 and 30 days"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_link_format() {
        assert_eq!(
            verify_link_format(
                "https://www.swisstransfer.com/d/3215702a-bed4-4cec-9eb6-d731048a2312"
            ),
            true
        );
        assert_eq!(
            verify_link_format(
                "https://www.swisstransfer.com/d/3215702a-bed4-4cec-9eb6-d731048a231"
            ),
            false
        );
        assert_eq!(
            verify_link_format(
                "https://www.swisstransfer.com/d/3215702a-bed4-4cec-9eb6-d731048a2312/"
            ),
            false
        );
        assert_eq!(
            verify_link_format(
                "https://www.swisstransfer.com/f/3215702a-bed4-4cec-9eb6-d731048a2312"
            ),
            false
        );
    }
}
