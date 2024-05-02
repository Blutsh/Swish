use std::{
    io::{Read, Write},
    sync::{Arc, Mutex},
};

use curl::easy::{Handler, ReadError, WriteError};
use indicatif::ProgressBar;

pub struct UploadHandler<R: Read> {
    pub reader: R,
    pub progress: Arc<Mutex<ProgressBar>>,
}

impl<R: Read> Handler for UploadHandler<R> {
    fn read(&mut self, data: &mut [u8]) -> Result<usize, ReadError> {
        let len = self.reader.read(data).map_err(|_| ReadError::Pause)?;
        self.progress.lock().unwrap().inc(len as u64);
        Ok(len)
    }
}

pub struct DownloadHandler<W: Write> {
    pub writer: W,
    pub progress: Arc<Mutex<ProgressBar>>,
}

impl<W: Write> Handler for DownloadHandler<W> {
    fn write(&mut self, data: &[u8]) -> Result<usize, WriteError> {
        self.writer.write(data).map_err(|_| WriteError::Pause)?;
        self.progress.lock().unwrap().inc(data.len() as u64);
        Ok(data.len())
    }
}

pub struct DataHandler {
    pub data: Vec<u8>,
}

impl DataHandler {
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }
}

impl Handler for DataHandler {
    fn write(&mut self, data: &[u8]) -> Result<usize, WriteError> {
        self.data.extend_from_slice(data);
        Ok(data.len())
    }
}