use std::path::Path;
use std::io::{Result, Write, Read};
use std::fs::{create_dir_all, File};

use crate::util;

use serde::{Serialize, Deserialize};
use tempfile::NamedTempFile;
use filetime::{set_file_mtime, FileTime};
use std::cmp::min;
use crate::util::{convert_error, ReadWrite};
use serde::de::DeserializeOwned;

use crate::tree::Manifest;
use std::time::SystemTime;
use std::marker::PhantomData;


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

pub(crate) struct CommandTransmitter<'a, R: Read, W: Write, RW: ReadWrite<R, W>> {
    root: &'a Path,
    io: &'a mut RW,
    p1: PhantomData<&'a R>,
    p2: PhantomData<&'a W>
}

impl<R: Read, W: Write, RW: ReadWrite<R, W>> CommandTransmitter<'_, R, W, RW> {
    pub fn new<'a>(root: &'a Path, io: &'a mut RW) -> CommandTransmitter<'a, R, W, RW> {
        CommandTransmitter {
            root,
            io,
            p1: PhantomData,
            p2: PhantomData
        }
    }
}


pub(crate) fn command_handler_loop<R: Read, W: Write, RW: ReadWrite<R, W>>(root: &Path, manifest: &Manifest, mut io: RW) -> Result<()> {
    loop {
        let next = read_bincoded(io.as_reader())?;
        match next {
            Command::End => {
                eprintln!("Received End command, exiting");
                return Ok(())
            },
            Command::SendManifest => {
                eprintln!("Received SendManifest command");
                write_bincoded(io.as_writer(), &manifest)?;
            }
            Command::SendFile(path) => {
                eprintln!("Received SendFile command for {:?}", &path);
                let mut file = root.to_owned();
                for segment in path {
                    file.push(segment);
                }
                let meta = file.metadata()?;
                let size = meta.len();
                let mtime = PortableTime::new(meta.modified()?);
                let mut file = File::open(file)?;
                let output = io.as_writer();
                write_bincoded(output, &mtime)?;
                write_bincoded(output, &size)?;
                std::io::copy(&mut file, output)?;
            }
        }

        io.as_writer().flush()?;
    }
}

impl<'a, R: Read, W: Write, RW: ReadWrite<R, W>> Transmitter for CommandTransmitter<'a, R, W, RW> {
    fn transmit(&mut self, path: &Path) -> Result<()> {
        let mut args = Vec::new();
        for comp in path.iter() {
            args.push(String::from(comp.to_string_lossy()));
        }

        write_bincoded(self.io.as_writer(), &Command::SendFile(args))?;

        let time = read_bincoded::<R, PortableTime>(self.io.as_reader())?.to_file_time();
        let size = read_bincoded(self.io.as_reader())?;

        let path = self.root.join(path);
        save_copy(&path, self.io.as_reader(), size)?;
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
