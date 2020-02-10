use std::io::Error;

mod config;
mod tree;
mod util;
mod file_transfer;

fn main() -> Result<(), Error> {
    let cfg = config::configure()?;
    let target_root = cfg.target.canonicalize()?;
    let source_root = cfg.source.canonicalize()?;
    let transmitter = file_transfer::LocalTransmitter::new(&source_root, &target_root);

    let source = tree::Manifest::create_persistent(source_root.as_path(), &cfg)?;
    let destination = tree::Manifest::create_ephemeral(target_root.as_path(), &cfg)?;
    destination.copy_from(&source, &transmitter, &cfg)?;
    Ok(())
}
