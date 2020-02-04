use std::io::Error;
use crate::config::ManifestMode;


mod config;
mod tree;


fn main() -> Result<(), Error>{
    let cfg = config::config()?;
    let src = cfg.source.canonicalize()?;
    tree::DirectoryEntry::new(src)?;
    Ok(())
}
