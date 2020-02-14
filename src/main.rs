use std::io::{Error, stdin, stdout, ErrorKind};
use std::process::exit;

use bincode::Config;

use serde::{Serialize, Deserialize};

use crate::config::{Configuration, ProcessRole};
use std::path::PathBuf;

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

fn main_as_sender(cfg: &Configuration) -> Result<(), Error> {
    let transmitter = cfg.transmitter();
    let transmitter = transmitter.as_ref();
    let manifest = transmitter.produce_source_manifest(cfg)?;
    let mut input = stdin();
    let mut output = stdout();
    let convert_error = |e| Error::new(ErrorKind::Other, e);

    loop {
        let next: Command = bincode::deserialize_from(&mut input).map_err(convert_error)?;

        match next {
            Command::End => return Ok(()),
            Command::SendManifest =>
                bincode::serialize_into(&mut output, &manifest).map_err(convert_error)?,
            Command::SendFile(path) =>
                transmitter.transmit(&PathBuf::from(path))?
        }
    }
}

fn main_as_receiver(cfg: &Configuration) -> Result<(), Error> {
    unimplemented!()
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
            main_as_sender(&cfg),
        Some(ProcessRole::Receiver) =>
            main_as_receiver(&cfg),
        _ =>
            main_as_controller(&cfg)
    }
}
