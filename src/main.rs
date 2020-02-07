use std::io::Error;

mod config;
mod tree;
mod util;
/*

fn copy_file<P: AsRef<Path>>(target: P, source: P) -> Result<u64, Error> {
    std::fs::copy(source, target)
}

fn push(base: &Path, name: &str) -> PathBuf {
    let mut p = PathBuf::from(base);
    p.push(name);
    p
}

fn copy(target_dir: &Path, source_dir: &Path, target: &mut DirectoryEntry, source: &DirectoryEntry, cfg: &Configuration) -> Result<(), Error> {
    for dir in &source.subdirs {
        let target_subdir = push(target_dir, &dir.name);
        let source_subdir = push(source_dir, &dir.name);
        let partner = find_named_mut(&mut target.subdirs, &dir.name);

        let partner = match partner {
            None => {
                create_dir(target_subdir.as_path())?;
                target.subdirs.push(DirectoryEntry::empty(target_subdir.as_path()));
                target.subdirs.last_mut().unwrap()
            }
            Some(p) => p,
        };

        if partner.hash_value != dir.hash_value {
            if cfg.verbose {
                println!("Descending into {}", target_subdir.to_string_lossy());
            }
            copy(target_subdir.as_path(), source_subdir.as_path(), partner, dir, &cfg)?;
        }
    }

    for file in &source.files {
        let target_file = push(target_dir, &file.name);
        let source_file = push(source_dir, &file.name);
        let partner = find_named_mut(&mut target.files, &file.name);

        if partner.is_none() || {
            let partner = partner.unwrap();
            partner.modification_time != file.modification_time ||
                partner.file_size != file.file_size ||
                partner.hash_value != file.hash_value
        } {
            if cfg.verbose {
                println!("Copying file {} to {}", source_file.to_string_lossy(), target_file.to_string_lossy())
            }
            copy_file(target_file.as_path(), source_file.as_path())?;
        }
    }

    Ok(())
}
*/
fn main() -> Result<(), Error> {
    let cfg = config::configure()?;
    let src = cfg.source.canonicalize()?;
    let src_manifest = tree::Manifest::create_persistent(src.as_path(), &cfg)?;

    let dst = cfg.target.canonicalize()?;
    let mut dst_manifest = tree::Manifest::create_ephemeral(dst.as_path(), &cfg)?;
    /*copy(dst.as_path(),
         src.as_path(),
         &mut dst_manifest,
         &src_manifest.0,
         &cfg)?;
*/
    Ok(())
}
