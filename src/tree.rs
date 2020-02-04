use std::ffi::OsStr;
use std::fs::{File, read_dir};
use std::io::{Error, Read, empty, Chain, Cursor};
use std::path::Path;
use std::time::SystemTime;

use ring::digest::{Context, SHA256};

type ShaSum = [u8; 32];

#[derive(Debug)]
pub struct FileEntry {
    name: String,
    modification_time: SystemTime,
    hash_value: ShaSum,
}

impl FileEntry {
    pub fn new<S: AsRef<OsStr>>(path: S, verbose: bool) -> Result<FileEntry, Error> {
        let path = Path::new(&path);

        let hash_value = unsafe {
            let file = File::open(path)?;
            let mmap = memmap2::Mmap::map(&file)?;
            hash(mmap.as_ref())?
        };
        let modification_time = path.metadata()?.modified()?;

        let name = filename_to_string(path.file_name());

        if verbose {
            println!("Hashed file {} into {}", path.display(), hex::encode(&hash_value))
        }

        Ok(FileEntry {
            name,
            modification_time,
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
    pub fn new<S: AsRef<OsStr>>(path: S, verbose: bool) -> Result<DirectoryEntry, Error> {
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
                let subtree = DirectoryEntry::new(sub_path, verbose)?;
                let hc = subtree.hash_value;
                let name = String::from(&subtree.name);
                subdirs.push(subtree);
                hash_input.append(&mut name.as_bytes().to_vec());
                hash_input.append(&mut hc.to_vec());
            } else {
                let file = FileEntry::new(sub_path, verbose)?;
                let hc = file.hash_value;
                let name = String::from(&file.name);
                files.push(file);
                hash_input.append(&mut name.as_bytes().to_vec());
                hash_input.append(&mut hc.to_vec());
            }
        }


        let hash_value = hash(hash_input.as_ref())?;

        if verbose {
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