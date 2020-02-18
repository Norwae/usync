use std::io::{Error, Read, Write, stdin, stdout, ErrorKind};
use std::sync::mpsc::channel;

use crate::config::{Configuration, ProcessRole, PathDefinition};
use crate::tree::Manifest;
use crate::util::*;
use crate::file_transfer::*;
use std::process;
use std::thread;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::process::Stdio;

mod config;
mod tree;
mod util;
mod file_transfer;

fn non_local_path<A>(path: &PathDefinition) -> Result<A, Error> {
    Err(Error::new(ErrorKind::Other, format!("Non-local path where local context is required: {}", path)))
}

fn main_as_server(cfg: &Configuration) -> Result<(), Error> { // ! would be better, but hey...
    if let PathDefinition::Local(root) = cfg.source() {
        let manifest =
            Manifest::create_persistent(root, cfg.verbose(), cfg.hash_settings(), cfg.manifest_path())?;
        let manifest = Arc::new(manifest);
        let server_port = TcpListener::bind(format!("0.0.0.0:{}", cfg.server_port()))?;

        loop {
            let verbose = cfg.verbose();
            let (conn, sa) = server_port.accept()?;
            let manifest = manifest.clone();
            let root = PathBuf::from(root);
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
    } else {
        non_local_path(cfg.source())
    }

}



fn main_as_sender<R: Read, W: Write, RW: ReadWrite<R, W>>(cfg: &Configuration, io: RW) -> Result<(), Error> {
    if let PathDefinition::Local(root) = cfg.source() {
        let manifest = Manifest::create_persistent(
            &root,
            false,
            cfg.hash_settings(),
            cfg.manifest_path())?;

        command_handler_loop(&root, &manifest, io)
    } else {
        non_local_path(cfg.source())
    }
}

fn main_as_receiver<R: Read, W: Write, RW: ReadWrite<R, W>>(cfg: &Configuration, mut io: RW) -> Result<(), Error> {
    if let PathDefinition::Local(root) = cfg.target() {
        let local_manifest = Manifest::create_ephemeral(&root, false, cfg.hash_settings())?;
        write_bincoded(io.as_writer(), &Command::SendManifest)?;
        let remote_manifest = read_bincoded(io.as_reader())?;

        let mut transmitter = CommandTransmitter::new(&root, &mut io);
        local_manifest.copy_from(&remote_manifest, &mut transmitter, cfg.verbose())?;

        write_bincoded(io.as_writer(), &Command::End)
    } else {
        non_local_path(cfg.target())
    }
}

fn main_as_local(cfg: &Configuration) -> Result<(), Error> {
    if let PathDefinition::Local(to) = cfg.target() {
        if let PathDefinition::Local(from) = cfg.source() {
            let target = Manifest::create_ephemeral(&to, cfg.verbose(), cfg.hash_settings())?;
            let src = Manifest::create_persistent(&from, cfg.verbose(), cfg.hash_settings(), cfg.manifest_path())?;
            target.copy_from(&src, &mut LocalTransmitter::new(&from, &to), cfg.verbose())
        } else {
            non_local_path(cfg.source())
        }
    } else {
        non_local_path(cfg.target())
    }
}

fn main_as_local_pipe(cfg: &Configuration) -> Result<(), Error> {
    let c1 = cfg.clone();
    let c2 = cfg.clone();
    let (send_to_receiver, receive_from_sender) = channel();
    let (send_to_sender, receive_from_receiver) = channel();
    let sender = thread::spawn(move || {
        let output = SendAdapter::new(send_to_receiver);
        let input = ReceiveAdapter::new(receive_from_receiver);

        main_as_sender(&c1, CombineReadWrite::new(input, output)).unwrap_or_else(|e| {
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

fn build_command(cfg: &Configuration, role: &str, remote: &str, target_param: &str, target_path: &str) -> std::process::Command {
    let mode = cfg.hash_settings().manifest_mode().to_string();

    let mut ssh_invoke = vec![remote, "usync",
                              "--role", role,
                              target_param, target_path,
                              "--manifest-file", cfg.manifest_path().to_str().unwrap(),
                              "--hash-mode", &mode
    ];

    if cfg.hash_settings().force_rebuild() {
        ssh_invoke.push("--force-rebuild-manifest")
    }
    for p in cfg.hash_settings().exclude_patterns() {
        ssh_invoke.push("--exclude");
        ssh_invoke.push(p.as_str());
    }

    if cfg.verbose() {
        println!("Spawning process: {:?}", &ssh_invoke);
    }

    let mut proc = process::Command::new("ssh");
    proc
        .args(ssh_invoke)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    proc
}

fn main_as_controller(cfg: &Configuration) -> Result<(), Error> {
    let src = cfg.source();
    let trg = cfg.target();

    match (src, trg) {
        (PathDefinition::Local(_), PathDefinition::Local(_)) => {
            if cfg.force_pipeline() {
                main_as_local_pipe(cfg)
            } else {
                main_as_local(cfg)
            }
        },
        (PathDefinition::Server(remote), PathDefinition::Local(_)) => {
            let stream = TcpStream::connect(remote)?;
            main_as_receiver(cfg, stream)
        }
        (PathDefinition::Remote(remote, remote_path), PathDefinition::Local(_)) => {
            let mut cmd = build_command(cfg, "sender", remote, "--source", remote_path);
            let proc = cmd.spawn()?;
            let io = CombineReadWrite::new(proc.stdout.unwrap(), proc.stdin.unwrap());
            main_as_receiver(cfg, io)
        }
        (PathDefinition::Local(_), PathDefinition::Remote(remote, remote_path)) => {
            let mut cmd = build_command(cfg, "receiver", remote, "--target", remote_path);
            let proc = cmd.spawn()?;
            let io = CombineReadWrite::new(proc.stdout.unwrap(), proc.stdin.unwrap());
            main_as_sender(cfg, io)
        }
        _ => Err(Error::new(ErrorKind::Other, format!("Unsupported combination of paths: {} vs {}", src, trg)))
    }
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
