use std::path::Path;
use std::io::Error;
use std::process::exit;


mod config;
mod tree;


fn main() -> Result<(), Error>{
    let cfg = config::configure()?;
    let src = cfg.source.canonicalize()?;
    tree::DirectoryEntry::new(src, cfg.verbose)?;
    Ok(())
}
