use std::path::{Path, PathBuf};
use std::io::{Result, Write, Read};
use std::fs::{create_dir_all, File, Metadata};

use crate::util;

use serde::{Serialize, Deserialize};
use tempfile::NamedTempFile;
use filetime::{set_file_mtime, FileTime};
use crate::util::convert_error;
use serde::de::DeserializeOwned;

use crate::tree::Manifest;
use std::time::SystemTime;

pub trait FileAccess {
    type Read : std::io::Read;
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

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    End,
    SendManifest,
    SendFile(PortablePath),
}

pub trait Transmitter {
    fn transmit(&mut self, path: &Path) -> Result<()>;
}

pub struct LocalTransmitter<'a> {
    source: &'a Path,
    target: &'a Path
}

impl LocalTransmitter<'_> {
    pub fn new<'a> (from: &'a Path, to: &'a Path) -> LocalTransmitter<'a> {
        LocalTransmitter {
            source: from,
            target: to
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

pub fn read_bincoded<R: Read, C: DeserializeOwned>(input: &mut R) -> Result<C> {
    bincode::deserialize_from(input).map_err(util::convert_error)
}

pub fn write_bincoded<W: Write, S: Serialize>(output: &mut W, data: &S) -> Result<()> {
    let bytes = bincode::serialize(data).map_err(util::convert_error)?;
    output.write_all(bytes.as_slice())
}

#[derive(Serialize, Deserialize,Debug,PartialEq,Eq)]
pub struct PortablePath {
    segments: Vec<String>
}

impl PortablePath {
    pub fn from<A: AsRef<Path>>(path: A) -> PortablePath {
        let path = path.as_ref();
        PortablePath {
            segments: path.into_iter()
                .map(|os| String::from(os.to_str().unwrap()))
                .collect::<Vec<String>>()
        }
    }

    pub fn relative_to(&self, root: &Path) -> PathBuf {
        let mut rv = root.to_owned();

        for s in &self.segments {
            rv.push(s);
        }

        rv
    }
}

#[derive(Deserialize, Serialize)]
struct FileAttributes {
    size: u64,
    secs: i64,
    nanos: u32
}

impl FileAttributes {
    fn new(size: u64, time: SystemTime) -> FileAttributes {
        let time = FileTime::from(time);
        FileAttributes {
            size,
            secs: time.unix_seconds(),
            nanos: time.nanoseconds()
        }
    }

    fn to_file_time(&self) -> FileTime {
        FileTime::from_unix_time(self.secs, self.nanos)
    }
}

pub struct CommandTransmitter<'a,  RW: Read + Write> {
    root: &'a Path,
    io: &'a mut RW
}

impl<RW: Read + Write> CommandTransmitter<'_, RW> {
    pub fn new<'a>(root: &'a Path, io: &'a mut RW) -> CommandTransmitter<'a, RW> {
        CommandTransmitter {
            root,
            io
        }
    }
}


pub(crate) fn command_handler_loop<RW: Read + Write, A: FileAccess>(root: &Path, manifest: &Manifest, mut io: RW, access: &A) -> Result<()> {
    let io = &mut io;
    loop {
        let next = read_bincoded(io)?;
        match next {
            Command::End => {
                return Ok(())
            },
            Command::SendManifest => {
                write_bincoded(io, &manifest)?;
            }
            Command::SendFile(path) => {
                let file = path.relative_to(root);
                let meta = access.metadata(&file)?;
                let attrs = FileAttributes::new(meta.len(), meta.modified()?);
                let mut reader = access.read(&file)?;

                write_bincoded( io, &attrs)?;
                std::io::copy(&mut reader, io)?;
            }
        }

        io.flush()?;
    }
}

impl<'a, RW: Read + Write> Transmitter for CommandTransmitter<'a, RW> {
    fn transmit(&mut self, path: &Path) -> Result<()> {
        write_bincoded(self.io, &Command::SendFile(PortablePath::from(path)))?;

        let meta = read_bincoded::<RW, FileAttributes>(self.io)?;
        let path = self.root.join(path);

        save_copy(&path, self.io, meta.size)?;
        set_file_mtime(&path, meta.to_file_time())?;

        Ok(())
    }
}


fn save_copy<R: Read>(target: &Path, reader: &mut R, size: u64) -> Result<()> {
    let parent = target.parent().unwrap();
    if !parent.exists() {
        create_dir_all(parent)?;
    }

    let mut stage_file = NamedTempFile::new_in(parent)?;
    let mut reader = reader.take(size);

    std::io::copy(&mut reader, stage_file.as_file_mut())?;

    stage_file.persist(target).map_err(convert_error)?;
    Ok(())
}
