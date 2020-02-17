use std::env::{args, current_exe};
use std::ffi::OsStr;
use std::fs::File;
use std::io::{Error, ErrorKind, Read, stdin, stdout, Write};
use std::path::PathBuf;
use std::process::exit;
use std::sync::mpsc::channel;

use bincode::Config;
use memmap2::Mmap;
use serde::{Deserialize, Serialize};

use crate::config::{Configuration, ProcessRole};
use crate::tree::Manifest;
use std::thread;
use crate::util::{SendAdapter, ReceiveAdapter};

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
    let vector = bincode::serialize(data).map_err(util::convert_error)?;
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
    let mut v = vec![0u8;length as usize];
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

    loop {
        let command_buffer: Vec<u8> = read_sized(&mut input)?;
        let next = bincode::deserialize(command_buffer.as_slice()).map_err(util::convert_error)?;
        match next {
            Command::End => {
                return Ok(())
            },
            Command::SendManifest => {
                write_bincoded(&mut output, &manifest)?;
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

    let local_manifest = Manifest::create_ephemeral(root, false, cfg.hash_settings())?;
    write_bincoded(&mut output, &Command::SendManifest)?;
    read_manifest(&mut input);

    write_bincoded(&mut output, &Command::End)
}

fn read_manifest<R: Read>(mut input: &mut R) -> Result<Manifest, Error> {
    let remote_manifest = read_sized(&mut input)?;
    bincode::deserialize(remote_manifest.as_slice()).map_err(util::convert_error)
}

fn main_as_controller(cfg: &Configuration) -> Result<(), Error> {
    let c1 = cfg.clone();
    let c2 = cfg.clone();
    let (send_to_receiver, receive_from_sender) = channel();
    let (send_to_sender, receive_from_receiver) = channel();

    let sender = thread::spawn(move || {
        let output = SendAdapter::new(send_to_receiver);
        let input = ReceiveAdapter::new(receive_from_receiver);

        main_as_sender(&c1, input, output).unwrap_or_else(|e|{
            println!("Sender failed with: {}", e);
        });
    });

    let receiver = thread::spawn(move || {
        let output = SendAdapter::new(send_to_sender);
        let input = ReceiveAdapter::new(receive_from_sender);

        main_as_receiver(&c2, input, output).unwrap_or_else(|e| {
            println!("Receive failed: {}", e)
        });
    });

    sender.join().unwrap();
    receiver.join().unwrap();
    Ok(())
}

fn main() -> Result<(), Error> {
    let cfg = config::configure()?;
    match cfg.role() {
        Some(ProcessRole::Sender) =>
            main_as_sender(&cfg, stdin(), stdout()),
        Some(ProcessRole::Receiver) =>
            main_as_receiver(&cfg, stdin(), stdout()),
        _ =>
            main_as_controller(&cfg)
    }
}
