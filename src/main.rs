use std::fs::File;
use std::io::{Error, Read, Write, stdin, stdout, BufWriter, BufReader};
use std::sync::mpsc::channel;

use crate::config::{Configuration, ProcessRole};
use crate::tree::Manifest;
use crate::util::{SendAdapter, ReceiveAdapter};
use crate::file_transfer::*;
use std::thread;
use filetime::FileTime;

mod config;
mod tree;
mod util;
mod file_transfer;

fn main_as_sender<R: Read, W: Write>(cfg: &Configuration, input: R, output: W) -> Result<(), Error> {
    let root = cfg.source();
    let manifest = Manifest::create_persistent(
        root,
        false,
        cfg.hash_settings(),
        cfg.manifest_path())?;
    let mut output = BufWriter::new(output);
    let mut input = BufReader::new(input);

    loop {

        let next = read_bincoded(&mut input)?;
        match next {
            Command::End => {
                return Ok(())
            },
            Command::SendManifest => {
                write_bincoded(&mut output, &manifest)?;
            }
            Command::SendFile(path) => {
                let file = root.join(path);
                let meta = file.metadata()?;
                let size = meta.len();
                let mtime = meta.modified()?;
                let mut file = File::open(file)?;
                let output = &mut output;
                let mtime = FileTime::from(mtime);
                write_size(output, mtime.unix_seconds() as u64)?;
                write_size(output, mtime.nanoseconds() as u64)?;
                write_size(output, size)?;
                std::io::copy(&mut file, output)?;
            }
        }

        output.flush()?;
    }
}

fn main_as_receiver<R: Read, W: Write>(cfg: &Configuration, mut input: R, mut output: W) -> Result<(), Error> {
    let root = cfg.target();

    let local_manifest = Manifest::create_ephemeral(root, false, cfg.hash_settings())?;
    write_bincoded(&mut output, &Command::SendManifest)?;
    let remote_manifest = read_bincoded(&mut input)?;

    let mut transmitter = CommandTransmitter::new(root, &mut input, &mut output)?;
    local_manifest.copy_from(&remote_manifest, &mut transmitter, cfg.verbose())?;

    write_bincoded(&mut output, &Command::End)
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
