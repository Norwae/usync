use std::path::{Path, PathBuf};
use std::io::{Result, Error, ErrorKind};
use crate::tree::Manifest;
use crate::config::Configuration;

pub trait Transmitter {
    fn transmit(&self, path: &Path) -> Result<()>;
    fn produce_source_manifest(&self, cfg: &Configuration) -> Result<Manifest>;
    fn produce_target_manifest(&self, cfg: &Configuration) -> Result<Manifest>;
}

pub struct LocalTransmitter {
    source: PathBuf,
    target: PathBuf
}

impl LocalTransmitter {
    pub fn new(cfg: &Configuration) -> LocalTransmitter {
        LocalTransmitter {
            source: cfg.source().to_owned(),
            target: cfg.target().to_owned()
        }
    }
}

impl Transmitter for LocalTransmitter {
    fn produce_source_manifest(&self, cfg: &Configuration) -> Result<Manifest> {
        Manifest::create_persistent(&self.source, cfg)
    }

    fn produce_target_manifest(&self, cfg: &Configuration) -> Result<Manifest> {
        Manifest::create_ephemeral(&self.target, cfg)
    }

    fn transmit(&self, path: &Path) -> Result<()> {
        let from = self.source.join(path);
        let to = self.target.join(path);

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