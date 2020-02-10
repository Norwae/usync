use std::path::{Path, PathBuf};
use std::io::{Result, Error};

pub trait Transmitter {
    fn transmit(&self, path: &Path) -> Result<()>;
}

pub struct LocalTransmitter<'a> {
    source_root: &'a Path,
    target_root: &'a Path
}

impl <'a> LocalTransmitter<'a> {
    pub(crate) fn new(source_root: &'a Path, target_root: &'a Path) -> LocalTransmitter<'a>{
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
        std::fs::create_dir_all(&to.parent().unwrap())?;
        std::fs::copy(from, &to)?;
        filetime::set_file_mtime(&to, time)?;
        Ok(())
    }
}