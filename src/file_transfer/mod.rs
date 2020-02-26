use std::path::Path;
use std::io::Result;
use std::fs::{create_dir_all, File, Metadata};

pub mod local;
pub mod remote;

pub trait FileAccess {
    type Read: std::io::Read;
    fn metadata(&self, path: &Path) -> Result<Metadata>;
    fn read(&self, path: &Path) -> Result<Self::Read>;
}

pub struct DefaultFileAccess;

impl FileAccess for DefaultFileAccess {
    type Read = File;

    fn metadata(&self, path: &Path) -> Result<Metadata> {
        path.metadata()
    }

    fn read(&self, path: &Path) -> Result<Self::Read> {
        File::open(path)
    }
}

pub trait Transmitter {
    fn transmit(&mut self, path: &Path) -> Result<()>;
}
