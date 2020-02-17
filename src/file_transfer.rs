use std::path::Path;
use std::io::{Result, Write, Read};
use std::fs::create_dir_all;

use crate::util;

use serde::{Serialize, Deserialize};
use tempfile::NamedTempFile;
use filetime::{set_file_mtime, FileTime};
use std::cmp::min;
use crate::util::convert_error;
use serde::de::DeserializeOwned;


#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    End,
    SendManifest,
    SendFile(String),
}

pub trait Transmitter {
    fn transmit(&mut self, path: &Path) -> Result<()>;
}


pub fn read_bincoded<R: Read, C: DeserializeOwned>(input: &mut R) -> Result<C> {
    let command_buffer = read_sized(input)?;
    bincode::deserialize(command_buffer.as_slice()).map_err(util::convert_error)
}


pub fn write_bincoded<W: Write, S: Serialize>(output: &mut W, data: &S) -> Result<()> {
    let vector = bincode::serialize(data).map_err(util::convert_error)?;
    write_sized(output, vector)
}

pub fn write_sized<W: Write, O: AsRef<[u8]>>(output: &mut W, data: O) -> Result<()> {
    let r = data.as_ref();
    write_size(output, r.len() as u64)?;
    output.write_all(r)
}

pub fn write_size<W: Write>(output: &mut W, size: u64) -> Result<()> {
    output.write_all(&size.to_le_bytes())
}

pub fn read_size<R: Read>(input: &mut R) -> Result<u64> {
    let mut length_buffer = [0u8; 8];
    input.read_exact(&mut length_buffer)?;
    Ok(u64::from_le_bytes(length_buffer))
}

pub fn read_sized<R: Read>(input: &mut R) -> Result<Vec<u8>> {
    let length = read_size(input)?;
    let mut v = vec![0u8; length as usize];
    input.read_exact(v.as_mut_slice())?;

    Ok(v)
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

pub(crate) struct CommandTransmitter<'a, R: Read, W: Write> {
    root: &'a Path,
    input: &'a mut R,
    output: &'a mut W,
}

impl<R: Read, W: Write> CommandTransmitter<'_, R, W> {
    pub fn new<'a>(root: &'a Path, input: &'a mut R, output: &'a mut W) -> Result<CommandTransmitter<'a, R, W>> {
        Ok(CommandTransmitter { root, input, output })
    }
}

impl<'a, R2: Read, W2: Write> Transmitter for CommandTransmitter<'a, R2, W2> {
    fn transmit(&mut self, path: &Path) -> Result<()> {
        write_bincoded(self.output, &Command::SendFile(path.to_string_lossy().into()))?;
        let sec = read_size(self.input)?;
        let nano = read_size(self.input)?;
        let size = read_size(self.input)?;
        let time = FileTime::from_unix_time(sec as i64, nano as u32);

        let path = self.root.join(path);
        save_copy(&path, &mut self.input, size)?;
        set_file_mtime(path, time)?;

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
