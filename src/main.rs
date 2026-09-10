// Hey there!
// As you can see, Im a real noob in Rust and dev in general, so please be kind with me.
// I hope someone with no skill issues could refactor the wole code base and make it readable and maintainable.
// Sorry for the mess x) at least it seems to work for now \o/

mod api;
mod email;
mod errors;
mod swissfiles;
use std::path::PathBuf;
use std::sync::OnceLock;
use log::LevelFilter;
use simple_logger::SimpleLogger;
use swissfiles::uploadparameters::UploadParameters;
use swissfiles::Swissfiles;

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

    /// Sender address for uploads, confirmed once by a code SwissTransfer emails you
    #[arg(short, long, value_name = "you@example.com")]
    email: Option<String>,

    /// Define the message for the file(s) uploaded
    #[arg(short, long, value_name = "Hello World")]
    message: Option<String>,

    /// Define the max number of downloads for the file(s) uploaded
    #[arg(short, long, value_name = "250", value_parser = validate_number_download)]
    number_download: Option<String>,

    /// Define the number of days the file(s) will be available for download
    #[arg(short, long, value_name = "30", value_parser = validate_duration)]
    duration: Option<String>,

    /// Define an output directory for the downloaded files
    #[arg(short, long, value_name = "output")]
    output: Option<String>,

    /// Enable verbose mode
    #[arg(short, long)]
    verbose: bool,
}

fn main() {
    if let Err(error) = run() {
        // `SwishError` has useful messages, but returning it from `main` would print
        // the `Debug` form and hide every one of them.
        eprintln!("Error: {}", error);
        std::process::exit(1);
    }
}

fn run() -> Result<(), SwishError> {
    let cli = Cli::parse();

    // Initialize logger
    let mut logger = SimpleLogger::new();
    if cli.verbose {
        logger = logger.with_level(LevelFilter::Debug);
    } else {
        logger = logger.with_level(LevelFilter::Info);
    }
    logger.init().unwrap();


    let arg = cli.file;


    //check if the arg is a link
    if let Some(link_uuid) = swisstransfer_link_uuid(&arg) {
        //Construct the swissfiles from the link
        let swissfiles = Swissfiles::new_remotefiles(&link_uuid, cli.password.as_deref())?;

        //Download the files
        swissfiles.download(cli.output.map(PathBuf::from).as_ref())?;

        return Ok(());
    }
    //check if the arg is a path
    if path_exists(&arg) {
        let path = PathBuf::from(&arg);
        let mut params = UploadParameters::default();

        if let Some(email) = cli.email {
            params.author_email = Some(email);
        }

        if let Some(password) = cli.password {
            params.password = password;
        }

        if let Some(message) = cli.message {
            params.message = message;
        }

        if let Some(number_download) = cli.number_download {
            params.number_of_download = number_download.parse().unwrap();
        }

        if let Some(duration) = cli.duration {
            params.duration = duration.parse().unwrap();
        }

        let local_files = Swissfiles::new_localfiles(path, &params)?;
        let download_link = local_files.upload()?;
        println!("Download link: {}", download_link);

        return Ok(());
    }

    Err(SwishError::InvalidUrl { url: arg })
}

/// Pull the transfer identifier out of a share URL.
///
/// SwissTransfer hands out `/dl/<uuid>` links today and used to hand out `/d/<uuid>`.
/// Both are accepted here, and both are then requested as `/dl/`, since the old path
/// no longer resolves. Returns `None` for anything that is not a share link.
fn swisstransfer_link_uuid(link: &str) -> Option<String> {
    static LINK_RE: OnceLock<Regex> = OnceLock::new();
    let re = LINK_RE.get_or_init(|| {
        Regex::new(
            r"(?i)^https://(?:www\.)?swisstransfer\.com/dl?/([a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})/?(?:[?#].*)?$",
        )
        .unwrap()
    });

    re.captures(link.trim())
        .map(|caps| caps[1].to_lowercase())
}

fn path_exists(path: &str) -> bool {
    //str is a file or folder
    PathBuf::from(path).exists()
}

/// Values the API accepts, rejected here so a typo does not cost a round trip.
const ALLOWED_DOWNLOADS: [u16; 5] = [1, 20, 100, 200, 250];
const ALLOWED_DURATIONS: [u32; 5] = [1, 3, 7, 15, 30];

fn validate_number_download(val: &str) -> Result<String, String> {
    let number = val.parse::<u16>().map_err(|_| "Must be a valid number")?;
    if ALLOWED_DOWNLOADS.contains(&number) {
        Ok(val.to_string())
    } else {
        Err(String::from(
            "Number of downloads must be 1, 20, 100, 200 or 250",
        ))
    }
}

fn validate_duration(val: &str) -> Result<String, String> {
    let number = val.parse::<u32>().map_err(|_| "Must be a valid number")?;
    if ALLOWED_DURATIONS.contains(&number) {
        Ok(val.to_string())
    } else {
        Err(String::from("Duration must be 1, 3, 7, 15 or 30"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "01a08a77-9bb1-7178-a8d8-200764814311";

    #[test]
    fn accepts_current_and_legacy_links() {
        let accepted = [
            format!("https://www.swisstransfer.com/dl/{}", UUID),
            format!("https://swisstransfer.com/dl/{}", UUID),
            format!("https://www.swisstransfer.com/d/{}", UUID),
            format!("https://www.swisstransfer.com/dl/{}/", UUID),
            format!("https://www.swisstransfer.com/dl/{}?lang=en", UUID),
            format!("https://www.swisstransfer.com/dl/{}#top", UUID),
            format!("  https://www.swisstransfer.com/dl/{}  ", UUID),
            format!("https://www.swisstransfer.com/dl/{}", UUID.to_uppercase()),
        ];

        for link in accepted {
            assert_eq!(
                swisstransfer_link_uuid(&link).as_deref(),
                Some(UUID),
                "should have accepted {}",
                link
            );
        }
    }

    #[test]
    fn rejects_everything_else() {
        let rejected = [
            format!("http://www.swisstransfer.com/dl/{}", UUID),
            format!("https://www.swisstransfer.ch/dl/{}", UUID),
            format!("www.swisstransfer.com/dl/{}", UUID),
            format!("https://www.swisstransfer.com/{}", UUID),
            format!("https://evil.com/dl/{}", UUID),
            format!("https://www.swisstransfer.com.evil.com/dl/{}", UUID),
            "https://www.swisstransfer.com/dl/not-a-uuid".to_string(),
        ];

        for link in rejected {
            assert_eq!(
                swisstransfer_link_uuid(&link),
                None,
                "should have rejected {}",
                link
            );
        }
    }

    #[test]
    fn test_path_exists() {
        let path = "Cargo.toml";
        assert!(path_exists(path));
        let path = "Cargo.toml2";
        assert!(!path_exists(path));
    }

    #[test]
    fn test_validate_number_download() {
        let number = "250";
        assert_eq!(validate_number_download(number), Ok(number.to_string()));
        for number in ["251", "0", "5"] {
            assert_eq!(
                validate_number_download(number),
                Err(String::from(
                    "Number of downloads must be 1, 20, 100, 200 or 250"
                )),
                "should have rejected {}",
                number
            );
        }
        let number = "a";
        assert_eq!(validate_number_download(number), Err(String::from("Must be a valid number")));
    }

    #[test]
    fn test_validate_duration() {
        let duration = "30";
        assert_eq!(validate_duration(duration), Ok(duration.to_string()));
        assert_eq!(validate_duration("3"), Ok(String::from("3")));
        for duration in ["31", "0", "2"] {
            assert_eq!(
                validate_duration(duration),
                Err(String::from("Duration must be 1, 3, 7, 15 or 30")),
                "should have rejected {}",
                duration
            );
        }
        let duration = "a";
        assert_eq!(validate_duration(duration), Err(String::from("Must be a valid number")));
    }
  
}
