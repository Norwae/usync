use std::path::Path;
use std::io::{Result, Write, Read};
use std::fs::{create_dir_all, File};

use crate::util;

use serde::{Serialize, Deserialize};
use tempfile::NamedTempFile;
use filetime::{set_file_mtime, FileTime};
use std::cmp::min;
use crate::util::convert_error;
use serde::de::DeserializeOwned;

use crate::tree::Manifest;
use std::time::SystemTime;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    End,
    SendManifest,
    SendFile(Vec<String>),
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
    bincode::serialize_into(output, data).map_err(util::convert_error)
}

#[derive(Deserialize, Serialize)]
struct PortableTime {
    secs: i64,
    nanos: u32
}

impl PortableTime {
    fn new(time: SystemTime) -> PortableTime {
        let time = FileTime::from(time);
        PortableTime {
            secs: time.unix_seconds(),
            nanos: time.nanoseconds()
        }
    }

    fn to_file_time(&self) -> FileTime {
        FileTime::from_unix_time(self.secs, self.nanos)
    }
}

struct LimitRead<'a, A: Read> {
    reader: &'a mut A,
    limit: usize,
}

impl<A: Read> Read for LimitRead<'_, A> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let m = min(self.limit, buf.len());
        Ok(if m == 0 {
            0usize
        } else {
            let b2 = &mut buf[..m];
            let got = self.reader.read(b2)?;
            self.limit -= got;
            got as usize
        })
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


pub(crate) fn command_handler_loop<RW: Read + Write>(root: &Path, manifest: &Manifest, mut io: RW) -> Result<()> {
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
                let mut file = root.to_owned();
                for segment in path {
                    file.push(segment);
                }
                let meta = file.metadata()?;
                let size = meta.len();
                let mtime = PortableTime::new(meta.modified()?);
                let mut file = File::open(file)?;
                write_bincoded( io, &mtime)?;
                write_bincoded( io, &size)?;
                std::io::copy(&mut file, io)?;
            }
        }

        io.flush()?;
    }
}

impl<'a, RW: Read + Write> Transmitter for CommandTransmitter<'a, RW> {
    fn transmit(&mut self, path: &Path) -> Result<()> {
        let mut args = Vec::new();
        for comp in path.iter() {
            args.push(String::from(comp.to_string_lossy()));
        }

        write_bincoded(self.io, &Command::SendFile(args))?;

        let time = read_bincoded::<RW, PortableTime>(self.io)?.to_file_time();
        let size = read_bincoded(self.io)?;

        let path = self.root.join(path);
        save_copy(&path, self.io, size)?;
        set_file_mtime(&path, time)?;

        Ok(())
    }
}


fn save_copy<R: Read>(target: &Path, reader: &mut R, size: u64) -> Result<()> {
    let parent = target.parent().unwrap();
    if !parent.exists() {
        create_dir_all(parent)?;
    }

    let mut stage_file = NamedTempFile::new_in(parent)?;
    let mut reader = LimitRead { reader, limit: size as usize };

    std::io::copy(&mut reader, stage_file.as_file_mut())?;

    stage_file.persist(target).map_err(convert_error)?;
    Ok(())
}
