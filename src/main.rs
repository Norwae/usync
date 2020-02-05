use std::io::Error;

mod config;
mod tree;


fn main() -> Result<(), Error>{
    let cfg = config::configure()?;
    let src = cfg.source.canonicalize()?;
    tree::DirectoryEntry::new(src, &cfg)?;
    Ok(())
}
