use std::{
    io::Write,
    sync::{Arc, Mutex},
};

use curl::easy::{Handler, WriteError};
use indicatif::ProgressBar;

pub struct DownloadHandler<W: Write> {
    pub writer: W,
    pub progress: Arc<Mutex<ProgressBar>>,
}

impl<W: Write> Handler for DownloadHandler<W> {
    fn write(&mut self, data: &[u8]) -> Result<usize, WriteError> {
        self.writer.write_all(data).map_err(|_| WriteError::Pause)?;
        self.progress.lock().unwrap().inc(data.len() as u64);
        Ok(data.len())
    }
}

pub struct DataHandler {
    pub data: Vec<u8>,
    /// Raw response header lines, kept so callers can read `Set-Cookie` back out.
    pub headers: Vec<String>,
}

impl DataHandler {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            headers: Vec::new(),
        }
    }
}

impl Default for DataHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl Handler for DataHandler {
    fn write(&mut self, data: &[u8]) -> Result<usize, WriteError> {
        self.data.extend_from_slice(data);
        Ok(data.len())
    }

    fn header(&mut self, data: &[u8]) -> bool {
        // Header lines are latin-1 in the worst case, so never fail on them.
        let line = String::from_utf8_lossy(data).trim_end().to_string();
        if !line.is_empty() {
            self.headers.push(line);
        }
        true
    }
}
