use std::io::Error;
use crate::tree::{DirectoryEntry, Named};
use std::path::{Path, PathBuf};
use std::fs::create_dir;

mod config;
mod tree;

fn find_named<'a, T : Named>(all: &'a mut [T], name: &String) -> Option<&'a mut T> {
    for candidate in all {
        if candidate.name() == name {
            return Some(candidate)
        }
    }
    None
}
fn copy_file<P: AsRef<Path>>(target: P, source: P) -> Result<u64, Error> {
    std::fs::copy(source, target)
}

fn copy(root: &Path, target: &mut DirectoryEntry, source: &DirectoryEntry) -> Result<(), Error> {
    for dir in &source.subdirs {
        let mut subdir = PathBuf::from(root);
        subdir.push(&dir.name);
        let subdir = subdir.as_path();
        let partner = find_named(&mut target.subdirs, &dir.name);

        let partner= match partner {
            None => {
                create_dir(subdir)?;
                target.subdirs.push(DirectoryEntry::empty(subdir));
                target.subdirs.last_mut().unwrap()
            },
            Some(p) => p,
        };

        if partner.hash_value != dir.hash_value {
            copy(subdir, partner, dir)?;
        }
    }

    for srcFile in &source.files {
        let mut file = PathBuf::from(root);
        file.push(&srcFile.name);
        let file = file.as_path();
        let partner = find_named(&mut target.files, &srcFile.name);

        if partner.is_none() || {
            let partner = partner.unwrap();
            partner.modification_time != srcFile.modification_time ||
            partner.file_size != srcFile.file_size ||
            partner.hash_value != srcFile.hash_value
        } {
            copy_file(file, srcFile.path.as_path())?;
        }
    }

    Ok(())
}

fn main() -> Result<(), Error>{
    let cfg = config::configure()?;
    let src = cfg.source.canonicalize()?;
    let src = tree::DirectoryEntry::new(src, &cfg)?;
    let dst = cfg.target.canonicalize()?;
    let mut dst = tree::DirectoryEntry::new(dst, &cfg)?;

    copy(cfg.target.as_path(), &mut dst, &src)?;


    Ok(())
}
