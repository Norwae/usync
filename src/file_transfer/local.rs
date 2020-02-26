use super::*;
use filetime::{set_file_mtime, FileTime};

pub struct LocalTransmitter<'a> {
    source: &'a Path,
    target: &'a Path,
}

impl LocalTransmitter<'_> {
    pub fn new<'a>(from: &'a Path, to: &'a Path) -> LocalTransmitter<'a> {
        LocalTransmitter {
            source: from,
            target: to,
        }
    }
}

impl Transmitter for LocalTransmitter<'_> {
    fn transmit(&mut self, path: &Path) -> Result<()> {
        let source = self.source.join(path);
        let target = self.target.join(path);
        let parent = target.parent().unwrap();

        if !parent.exists() {
            create_dir_all(parent)?;
        }

        std::fs::copy(&source, &target)?;
        let time = source.metadata()?.modified()?;
        set_file_mtime(&target, FileTime::from(time))?;
        Ok(())
    }
}
