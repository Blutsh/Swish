/// Hey there! 
/// As you can see, Im a real noob in Rust and dev in general, so please be kind with me.
/// I hope someone with no skill issues could refactor the wole code base and make it readable and maintainable.
/// Sorry for the mess x) at least it seems to work for now \o/

mod api;
mod errors;
mod swissfiles;
use std::path::PathBuf;
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

    /// Define the message for the file(s) uploaded
    #[arg(short, long, value_name = "Hello World")]
    message: Option<String>,

    /// Define the max number of downloads for the file(s) uploaded
    #[arg(short, long, value_name = "250", value_parser = validate_number_download)]
    number_download: Option<String>,

    /// Define the number of days the file(s) will be available for download
    #[arg(short, long, value_name = "30", value_parser = validate_duration)]
    duration: Option<String>,
}

fn main() -> Result<(), SwishError> {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .init()
        .unwrap();
    let cli = Cli::parse();
    let arg = cli.file;


    //check if the arg is a link
    if is_swisstransfer_link(&arg) {
        //Construct the swissfiles from the link
        let swissfiles = Swissfiles::new_remotefiles(&arg, cli.password.as_deref())?;

        //Download the files
        swissfiles.download(None)?;

        return Ok(());
    }
    //check if the arg is a path
    if path_exists(&arg) {
        let path = PathBuf::from(&arg);
        let mut params = UploadParameters::default();

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

fn is_swisstransfer_link(link: &str) -> bool {
    let re = Regex::new(r"^https://www\.swisstransfer\.com/d/[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$").unwrap();
    re.is_match(link)
}

fn path_exists(path: &str) -> bool {
    //str is a file or folder
    PathBuf::from(path).exists()
}

fn validate_number_download(val: &str) -> Result<String, String> {
    let number = val.parse::<u16>().map_err(|_| "Must be a valid number")?;
    if number < 1 || number > 250 {
        Err(String::from(
            "Number of downloads must be between 1 and 250",
        ))
    } else {
        Ok(val.to_string())
    }
}

fn validate_duration(val: &str) -> Result<String, String> {
    let number = val.parse::<u32>().map_err(|_| "Must be a valid number")?;
    if [1, 7, 15, 30].contains(&number) {
        Ok(val.to_string())
    } else {
        Err(String::from("Duration must be 1, 7, 15 or 30"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_swisstransfer_link() {
        let link = "https://www.swisstransfer.com/d/8b3b3b3b-3b3b-3b3b-3b3b-3b3b3b3b3b3b";
        assert_eq!(is_swisstransfer_link(link), true);
        let link = "http://www.swisstransfer.com/d/8b3b3b3b-3b3b-3b3b-3b3b-3b3b3b3b3b3b/";
        assert_eq!(is_swisstransfer_link(link), false);
        let link = "https://www.swisstransfer.ch/d/8b3b3b3b-3b3b-3b3b-3b3b-3b3b3b3b3b3b/";
        assert_eq!(is_swisstransfer_link(link), false);
        let link = "www.swisstransfer.com/d/8b3b3b3b-3b3b-3b3b-3b3b-3b3b3b3b3b3b";
        assert_eq!(is_swisstransfer_link(link), false);
        let link = "https://www.swisstransfer.com/8b3b3b3b-3b3b-3b3b-3b3b-3b3b3b3b3b3b";
        assert_eq!(is_swisstransfer_link(link), false);
    }

    #[test]
    fn test_path_exists() {
        let path = "Cargo.toml";
        assert_eq!(path_exists(path), true);
        let path = "Cargo.toml2";
        assert_eq!(path_exists(path), false);
    }

    #[test]
    fn test_validate_number_download() {
        let number = "250";
        assert_eq!(validate_number_download(number), Ok(number.to_string()));
        let number = "251";
        assert_eq!(
            validate_number_download(number),
            Err(String::from("Number of downloads must be between 1 and 250"))
        );
        let number = "0";
        assert_eq!(
            validate_number_download(number),
            Err(String::from("Number of downloads must be between 1 and 250"))
        );
        let number = "a";
        assert_eq!(validate_number_download(number), Err(String::from("Must be a valid number")));
    }

    #[test]
    fn test_validate_duration() {
        let duration = "30";
        assert_eq!(validate_duration(duration), Ok(duration.to_string()));
        let duration = "31";
        assert_eq!(
            validate_duration(duration),
            Err(String::from("Duration must be 1, 7, 15 or 30"))
        );
        let duration = "0";
        assert_eq!(
            validate_duration(duration),
            Err(String::from("Duration must be 1, 7, 15 or 30"))
        );
        let duration = "a";
        assert_eq!(validate_duration(duration), Err(String::from("Must be a valid number")));
    }
  
}
