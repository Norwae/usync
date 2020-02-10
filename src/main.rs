use std::io::Error;

mod config;
mod tree;
mod util;

fn main() -> Result<(), Error> {
    let cfg = config::configure()?;
    let src = cfg.source.canonicalize()?;
    let src_manifest = tree::Manifest::create_persistent(src.as_path(), &cfg)?;

    let dst = cfg.target.canonicalize()?;
    let mut dst_manifest = tree::Manifest::create_ephemeral(dst.as_path(), &cfg)?;
    dst_manifest.copy_from(&src_manifest, &cfg);
    Ok(())
}
