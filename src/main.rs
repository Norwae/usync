use std::io::Error;
use crate::filetransfer::LocalTransmitter;

mod config;
mod tree;
mod util;
mod filetransfer;

fn main() -> Result<(), Error> {
    let cfg = config::configure()?;
    let target_root = cfg.target.canonicalize()?;
    let source_root = cfg.source.canonicalize()?;
    let transmitter = LocalTransmitter::new(&source_root, &target_root);

    let source = tree::Manifest::create_persistent(source_root.as_path(), &cfg)?;
    let mut destination = tree::Manifest::create_ephemeral(source_root.as_path(), &cfg)?;
    destination.copy_from(&source, &transmitter, &cfg);
    Ok(())
}
