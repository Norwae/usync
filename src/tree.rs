use std::ffi::OsStr;
use std::fs::{File, read_dir};
use std::io::{Error, Read};
use std::path::Path;
use std::time::SystemTime;

use ring::digest::{Context, SHA256};
use crate::config::{ManifestMode, Configuration};

type ShaSum = [u8; 32];

#[derive(Debug)]
pub struct FileEntry {
    name: String,
    modification_time: SystemTime,
    file_size: u64,
    hash_value: ShaSum,
}


impl FileEntry {
    pub fn new<S: AsRef<OsStr>>(path: S, config: &Configuration) -> Result<FileEntry, Error> {
        let path = Path::new(&path);

        let hash_value = if config.manifest_mode == ManifestMode::Hash {
            unsafe {
                let file = File::open(path)?;
                let mmap = memmap2::Mmap::map(&file)?;
                hash(mmap.as_ref())?
            }
        } else {
            [0u8; 32]
        };
        let metadata = path.metadata()?;
        let modification_time = metadata.modified()?;
        let file_size = metadata.len();

        let name = filename_to_string(path.file_name());

        if config.verbose  {
            println!("Hashed file {} into {}", path.display(), hex::encode(&hash_value))
        }

        Ok(FileEntry {
            name,
            modification_time,
            file_size,
            hash_value,
        })
    }
}

#[derive(Debug)]
pub struct DirectoryEntry {
    name: String,
    modification_time: SystemTime,
    subdirs: Vec<DirectoryEntry>,
    files: Vec<FileEntry>,
    hash_value: ShaSum,
}

impl DirectoryEntry {
    pub fn new<S: AsRef<OsStr>>(path: S, config: &Configuration) -> Result<DirectoryEntry, Error> {
        let path = Path::new(&path);
        let dir = read_dir(path)?;
        let mut subdirs: Vec<DirectoryEntry> = Vec::new();
        let mut files: Vec<FileEntry> = Vec::new();
        let mut hash_input: Vec<u8> = Vec::new();
        let modification_time = path.metadata()?.modified()?;
        let name = filename_to_string(path.file_name());

        for entry in dir {
            let entry = entry?;
            let sub_path = path.join(entry.file_name());

            if entry.metadata()?.is_dir() {
                let subtree = DirectoryEntry::new(sub_path, config)?;
                hash_input.extend(subtree.name.as_bytes());
                hash_input.extend(&subtree.hash_value);
                subdirs.push(subtree);
            } else {
                let file = FileEntry::new(sub_path, config)?;
                hash_input.extend(file.name.as_bytes());
                hash_input.extend(&file.file_size.to_le_bytes());
                hash_input.extend(&file.hash_value);
                files.push(file);
            }
        }

        let hash_value = hash(hash_input.as_ref())?;

        if config.verbose {
            println!("Hashed directory {} into {}", path.display(), hex::encode(&hash_value))
        }

        Ok(DirectoryEntry {
            name,
            modification_time,
            subdirs,
            files,
            hash_value,
        })
    }
}

fn hash(input: &[u8]) -> Result<ShaSum, Error> {
    let mut sha256 = Context::new(&SHA256);
    let mut rv: ShaSum = [0u8; 32];
    sha256.update(input);
    sha256.finish().as_ref().read_exact(&mut rv)?;
    Ok(rv)
}

fn filename_to_string(filename: Option<&OsStr>) -> String {
    String::from(filename.unwrap().to_str().unwrap())
}