use std::fs::File;
use std::io::{Error, Read, Write, stdin, stdout};
use std::sync::mpsc::channel;

use crate::config::{Configuration, ProcessRole};
use crate::tree::Manifest;
use crate::util::*;
use crate::file_transfer::*;
use std::thread;
use filetime::FileTime;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod config;
mod tree;
mod util;
mod file_transfer;


fn command_handler_loop<R: Read, W: Write, RW: ReadWrite<R, W>>(root: &Path, manifest: &Manifest, mut io: RW) -> Result<(), Error> {

    loop {
        let next = read_bincoded(io.as_reader())?;
        match next {
            Command::End => {
                return Ok(())
            },
            Command::SendManifest => {
                write_bincoded(io.as_writer(), &manifest)?;
            }
            Command::SendFile(path) => {
                let file = root.join(path);
                let meta = file.metadata()?;
                let size = meta.len();
                let mtime = meta.modified()?;
                let mut file = File::open(file)?;
                let output = io.as_writer();
                let mtime = FileTime::from(mtime);
                write_size(output, mtime.unix_seconds() as u64)?;
                write_size(output, mtime.nanoseconds() as u64)?;
                write_size(output, size)?;
                std::io::copy(&mut file, output)?;
            }
        }

        io.as_writer().flush()?;
    }
}

fn main_as_server(cfg: &Configuration) -> Result<(), Error> { // ! would be better, but hey...
    let manifest =
        Manifest::create_persistent(cfg.source(), cfg.verbose(), cfg.hash_settings(), cfg.manifest_path())?;
    let manifest = Arc::new(manifest);
    let server_port = TcpListener::bind(format!("0.0.0.0:{}", cfg.server_port()))?;

    loop {
        let verbose = cfg.verbose();
        let (conn, sa) = server_port.accept()?;
        let manifest = manifest.clone();
        let root = PathBuf::from(cfg.source());
        if verbose {
            println!("Accepted connection {}", sa);
        }
        thread::spawn(move || {
            match command_handler_loop(&root, &manifest, conn) {
                Ok(_) => if verbose {
                    println!("Finished sending to {}", sa)
                },
                Err(err) => eprintln!("Command loop failed for {} with {}", sa, err),
            }
        });
    }
}



fn main_as_sender<R: Read, W: Write, RW: ReadWrite<R, W>>(cfg: &Configuration, io: RW) -> Result<(), Error> {
    let root = PathBuf::from(cfg.source());
    let manifest = Manifest::create_persistent(
        &root,
        false,
        cfg.hash_settings(),
        cfg.manifest_path())?;

    command_handler_loop(&root, &manifest, io)
}

fn main_as_receiver<R: Read, W: Write, RW: ReadWrite<R, W>>(cfg: &Configuration, mut io: RW) -> Result<(), Error> {
    let root = PathBuf::from(cfg.target());

    let local_manifest = Manifest::create_ephemeral(&root, false, cfg.hash_settings())?;
    write_bincoded(io.as_writer(), &Command::SendManifest)?;
    let remote_manifest = read_bincoded(io.as_reader())?;

    let mut transmitter = CommandTransmitter::new(&root, &mut io);
    local_manifest.copy_from(&remote_manifest, &mut transmitter, cfg.verbose())?;

    write_bincoded(io.as_writer(), &Command::End)
}

fn main_as_controller(cfg: &Configuration) -> Result<(), Error> {
    if cfg.source().starts_with("server:") {
        let remote = &cfg.source()[7..];
        let remote = TcpStream::connect(remote)?;
        return main_as_receiver(cfg, remote);
    }

    let c1 = cfg.clone();
    let c2 = cfg.clone();
    let (send_to_receiver, receive_from_sender) = channel();
    let (send_to_sender, receive_from_receiver) = channel();

    let sender = thread::spawn(move || {
        let output = SendAdapter::new(send_to_receiver);
        let input = ReceiveAdapter::new(receive_from_receiver);

        main_as_sender(&c1, CombineReadWrite::new(input, output)).unwrap_or_else(|e|{
            println!("Sender failed with: {}", e);
        });
    });

    let receiver = thread::spawn(move || {
        let output = SendAdapter::new(send_to_sender);
        let input = ReceiveAdapter::new(receive_from_sender);

        main_as_receiver(&c2, CombineReadWrite::new(input, output)).unwrap_or_else(|e| {
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
            main_as_sender(&cfg, CombineReadWrite::new(stdin(), stdout())),
        Some(ProcessRole::Receiver) =>
            main_as_receiver(&cfg, CombineReadWrite::new(stdin(), stdout())),
        Some(ProcessRole::Server) =>
            main_as_server(&cfg),
        _ =>
            main_as_controller(&cfg)
    }
}
