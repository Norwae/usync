use std::path::Path;
use std::io::{Result, Error, ErrorKind};

pub trait Transmitter {
    fn transmit(&self, path: &Path) -> Result<()>;
}

pub struct LocalTransmitter<'a> {
    source_root: &'a Path,
    target_root: &'a Path
}

impl <'a> LocalTransmitter<'a> {
    pub fn new(source_root: &'a Path, target_root: &'a Path) -> LocalTransmitter<'a>{
        LocalTransmitter {
            source_root,
            target_root
        }
    }
}

impl <'a> Transmitter for LocalTransmitter<'a> {
    fn transmit(&self, path: &Path) -> Result<()> {
        let from = self.source_root.join(path);
        let to = self.target_root.join(path);

        let metadata = from.metadata()?;
        let time = filetime::FileTime::from_last_modification_time(&metadata);
        let parent = match to.parent() {
            None => return Err(Error::new(ErrorKind::NotFound, "Could not create parent directory: No parent")),
            Some(p) => p,
        };

        std::fs::create_dir_all(parent)?;
        std::fs::copy(&from, &to)?;
        filetime::set_file_mtime(&to, time)?;
        Ok(())
    }
}