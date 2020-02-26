use std::path::{Path, PathBuf};
use std::io::{Result, Write, Read, BufReader, BufWriter};
use std::fs::{create_dir_all, File, Metadata};

use crate::util;

use serde::{Serialize, Deserialize};
use tempfile::NamedTempFile;
use filetime::{set_file_mtime, FileTime};
use serde::de::DeserializeOwned;

use crate::tree::Manifest;
use std::time::SystemTime;

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

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Command {
    End,
    SendManifest,
    SendFile(PortablePath),
}

pub trait Transmitter {
    fn transmit(&mut self, path: &Path) -> Result<()>;
}

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

fn read_bincoded<R: Read, C: DeserializeOwned>(input: R) -> Result<C> {
    bincode::deserialize_from(input).map_err(util::convert_error)
}

fn write_bincoded_with_flush<W: Write, S: Serialize>(mut output:  W, data: &S) -> Result<()> {
    write_bincoded(&mut output, data)?;
    output.flush()
}

fn write_bincoded<W: Write, S: Serialize>(mut output: &mut W, data: &S) -> Result<()>{
    bincode::serialize_into(&mut output, data).map_err(util::convert_error)
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct PortablePath {
    segments: Vec<String>
}

impl PortablePath {
    pub fn from<A: AsRef<Path>>(path: A) -> PortablePath {
        let path = path.as_ref();
        PortablePath {
            segments: path.into_iter()
                .map(|os| String::from(os.to_str().unwrap()))
                .collect()
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
    nanos: u32,
}

impl FileAttributes {
    fn new(size: u64, time: SystemTime) -> FileAttributes {
        let time = FileTime::from(time);
        FileAttributes {
            size,
            secs: time.unix_seconds(),
            nanos: time.nanoseconds(),
        }
    }

    fn to_file_time(&self) -> FileTime {
        FileTime::from_unix_time(self.secs, self.nanos)
    }
}

pub struct CommandTransmitter<R: Read, W: Write> {
    root: PathBuf,
    input: BufReader<R>,
    output: BufWriter<W>
}

impl<R: Read, W: Write> CommandTransmitter<R, W> {
    pub fn new(root: &Path, input: R, output: W) -> CommandTransmitter<R, W> {
        CommandTransmitter {
            root: root.to_owned(),
            input: BufReader::new(input),
            output: BufWriter::new(output)
        }
    }

    pub fn remote_manifest(&mut self) -> Result<Manifest> {
        write_bincoded_with_flush(&mut self.output, &Command::SendManifest)?;
        read_bincoded(&mut self.input)
    }
}

impl <R: Read, W: Write> Drop for CommandTransmitter<R, W> {
    fn drop(&mut self) {
        // if we can't politely send an end, well... tough
        let _ = write_bincoded_with_flush(&mut self.output, &Command::End);
    }
}


pub(crate) fn command_handler_loop<R: Read, W: Write, A: FileAccess>(root: &Path, manifest: &Manifest, input: R, output: W, access: &A) -> Result<()> {
    let mut input = BufReader::new(input);
    let mut output = BufWriter::new(output);
    loop {
        let next = read_bincoded(&mut input)?;
        match next {
            Command::End => {
                return Ok(());
            }
            Command::SendManifest => {
                write_bincoded_with_flush(&mut output, &manifest)?;
            }
            Command::SendFile(path) => {
                let file = path.relative_to(root);
                let meta = access.metadata(&file)?;
                let attrs = FileAttributes::new(meta.len(), meta.modified()?);
                let mut reader = access.read(&file)?;

                write_bincoded(&mut output, &attrs)?;
                std::io::copy(&mut reader, &mut output)?;
            }
        }

        output.flush()?;
    }
}

impl<R: Read, W: Write> Transmitter for CommandTransmitter<R, W> {
    fn transmit(&mut self, path: &Path) -> Result<()> {
        write_bincoded_with_flush(&mut self.output, &Command::SendFile(PortablePath::from(path)))?;

        let meta: FileAttributes = read_bincoded(&mut self.input)?;
        let path = self.root.join(path);

        save_file_with_tempfile(&path, &mut self.input, meta.size)?;
        set_file_mtime(&path, meta.to_file_time())?;

        Ok(())
    }
}

fn save_file_with_tempfile<R: Read>(target: &Path, reader: &mut R, size: u64) -> Result<()> {
    let parent = target.parent().unwrap();
    if !parent.exists() {
        create_dir_all(parent)?;
    }

    let mut stage_file = NamedTempFile::new_in(parent)?;
    let mut reader = reader.take(size);

    std::io::copy(&mut reader, stage_file.as_file_mut())?;

    stage_file.persist(target).map_err(|it|it.error)?;
    Ok(())
}
