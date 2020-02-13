use std::io::Error;

mod config;
mod tree;
mod util;
mod file_transfer;

fn main() -> Result<(), Error> {
    let cfg = config::configure()?;
    let transmitter = cfg.transmitter();

    let source = transmitter.produce_source_manifest(&cfg)?;
    let destination = transmitter.produce_target_manifest(&cfg)?;
    destination.copy_from(&source, transmitter.as_ref(), cfg.verbose())?;
    Ok(())
}
