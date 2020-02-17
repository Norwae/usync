use std::fs::File;
use std::io::{Error, Read, stdin, stdout, Write};
use std::sync::mpsc::channel;

use memmap2::Mmap;

use crate::config::{Configuration, ProcessRole};
use crate::tree::Manifest;
use std::thread;
use crate::util::{SendAdapter, ReceiveAdapter};
use crate::file_transfer::*;
use filetime::FileTime;

mod config;
mod tree;
mod util;
mod file_transfer;

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
                let mtime = file.metadata()?.modified()?;
                let file = File::open(file)?;
                let map = unsafe {
                    Mmap::map(&file)?
                };
                let output = &mut output;
                let mtime = FileTime::from(mtime);
                write_size(output, mtime.unix_seconds() as u64)?;
                write_size(output, mtime.nanoseconds() as u64)?;
                write_sized(output, map)?;
            }
        }
    }
}

fn main_as_receiver<R: Read, W: Write>(cfg: &Configuration, mut input: R, mut output: W) -> Result<(), Error> {
    let root = cfg.target();

    let local_manifest = Manifest::create_ephemeral(root, false, cfg.hash_settings())?;
    write_bincoded(&mut output, &Command::SendManifest)?;
    let remote_manifest = read_manifest(&mut input)?;

    let mut transmitter = CommandTransmitter::new(root, &mut input, &mut output)?;
    local_manifest.copy_from(&remote_manifest, &mut transmitter, cfg.verbose())?;

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
