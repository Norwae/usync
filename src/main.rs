use std::io::{Error, stdin, stdout, ErrorKind, Write, Read};
use std::process::exit;

use bincode::Config;

use serde::{Serialize, Deserialize};

use crate::config::{Configuration, ProcessRole};
use std::path::PathBuf;
use crate::tree::Manifest;
use memmap2::Mmap;
use std::fs::File;

mod config;
mod tree;
mod util;
mod file_transfer;

#[derive(Debug,PartialEq,Eq,Serialize,Deserialize)]
enum Command {
    End,
    SendManifest,
    SendFile(String)
}

fn write_bincoded<W: Write, S: Serialize>(output: &mut W, data: &S) -> Result<(), Error> {
    let vector = bincode::serialize(data)?;
    write_sized(output, vector)
}

fn write_sized<W: Write, O : AsRef<[u8]>>(output: &mut W, data: O) -> Result<(), Error> {
    let r = data.as_ref();
    output.write_all(&(r.len() as u64).to_le_bytes())?;
    output.write_all(r)
}

fn read_sized<R: Read>(input: &mut R) -> Result<Vec<u8>, Error> {
    let mut length_buffer = [0u8;8];
    input.read_exact(&mut length_buffer)?;
    let length = u64::from_le_bytes(length_buffer);
    let mut v = Vec::with_capacity(length as usize);
    input.read_exact(v.as_mut_slice())?;

    Ok(v)
}

fn main_as_sender<R: Read, W: Write>(cfg: &Configuration, mut input: R, mut output: W) -> Result<(), Error> {
    let root = cfg.source();
    let manifest = Manifest::create_persistent(
        root,
        false,
        cfg.hash_settings(),
        cfg.manifest_path())?;
    let convert_error = |e| Error::new(ErrorKind::Other, e);

    loop {
        let command_buffer: Vec<u8> = read_sized(&mut input)?;
        let next = bincode::deserialize(command_buffer.as_slice()).map_err(convert_error)?;
        match next {
            Command::End => return Ok(()),
            Command::SendManifest => {
                write_bincoded(&mut output, &manifest);
            }
            Command::SendFile(path) => {
                let file = root.join(path);
                let file = File::open(file)?;
                let map = unsafe {
                    Mmap::map(&file)?
                };
                write_sized(&mut output, map)?;
            }
        }
    }
}

fn main_as_receiver<R: Read, W: Write>(cfg: &Configuration, mut input: R, mut output: W) -> Result<(), Error> {
    let root = cfg.target();

    let manifest = Manifest::create_ephemeral(root, false, cfg.hash_settings())?;
    write_bincoded(&mut output, &Command::SendManifest)?;

    write_bincoded(&mut output, &Command::End)
}

fn main_as_controller(cfg: &Configuration) -> Result<(), Error> {
    let transmitter = cfg.transmitter();

    let source = transmitter.produce_source_manifest(&cfg)?;
    let destination = transmitter.produce_target_manifest(&cfg)?;
    destination.copy_from(&source, transmitter.as_ref(), cfg.verbose())?;
    Ok(())
}

fn main() -> Result<(), Error> {
    let cfg = config::configure()?;
    match cfg.role() {
        Some(ProcessRole::Sender) =>
            main_as_sender(&cfg, stdin(), stdout()),
        Some(ProcessRole::Receiver) =>
            main_as_receiver(&cfg),
        _ =>
            main_as_controller(&cfg)
    }
}
