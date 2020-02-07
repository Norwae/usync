use std::ffi::OsStr;
use std::fs::{File, read_dir};
use std::io::{Error, ErrorKind, Read, Result};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use ring::digest::{Context, SHA256};
use serde::{Serialize, Deserialize};

use crate::config::{ManifestMode, Configuration};

type ShaSum = [u8; 32];

pub(crate) trait Named {
    fn name(&self) -> &String;
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileEntry {
    pub(crate) name: String,
    pub(crate) modification_time: SystemTime,
    pub(crate) file_size: u64,
    pub(crate) hash_value: ShaSum,
}

impl Named for FileEntry {
    fn name(&self) -> &String {
        &self.name
    }
}


impl FileEntry {
    pub fn new<S: AsRef<OsStr>>(path: S, config: &Configuration) -> Result<FileEntry> {
        let path = PathBuf::from(&path);

        let hash_value = if config.manifest_mode == ManifestMode::Hash {
            unsafe {
                let file = File::open(path.as_path())?;
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

        if config.verbose {
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

#[derive(Debug, Serialize, Deserialize)]
pub struct DirectoryEntry {
    pub(crate) name: String,
    pub(crate) modification_time: SystemTime,
    pub(crate) subdirs: Vec<DirectoryEntry>,
    pub(crate) files: Vec<FileEntry>,
    pub(crate) hash_value: ShaSum,
}

impl Named for DirectoryEntry {
    fn name(&self) -> &String {
        &self.name
    }
}

#[derive(Serialize, Deserialize)]
pub struct Manifest(pub (crate) DirectoryEntry);

impl Manifest {
    fn manifest_file(root: &OsStr, cfg: &Configuration) -> PathBuf {
        let mut manifest_path = PathBuf::new();
        if cfg.manifest_path.is_absolute() {
            manifest_path.push(&cfg.manifest_path);
        } else {
            manifest_path.push(root);
            manifest_path.push(&cfg.manifest_path);
        }

        manifest_path
    }

    pub fn create<S: AsRef<OsStr>>(root: S, cfg: &Configuration) -> Result<Manifest> {
        let manifest_path = Manifest::manifest_file(root.as_ref(), cfg);

        if cfg.verbose {
            println!("Resolved manifest path to {}", manifest_path.as_path().to_string_lossy());
        }

        Manifest::_load(manifest_path.as_path()).or_else(|e| {
            if cfg.verbose {
                println!("Manifest file not usable: {}", e)
            }
            let de = DirectoryEntry::new(root, cfg);
            de.map(|e| Manifest(e))
        })
    }

    pub fn save<S: AsRef<OsStr>>(&self, root: S, cfg: &Configuration) -> Result<()> {
        let manifest_path = Manifest::manifest_file(root.as_ref(), cfg);
        let file = File::create(manifest_path.as_path())?;
        let r = bincode::serialize_into(file, self);
        r.map_err(|e2| Error::new(ErrorKind::Other, e2))?;

        if cfg.verbose {
            println!("Saved manifest file to {}", manifest_path.to_string_lossy());
        }

        Ok(())
    }

    fn _load<S: AsRef<Path>>(file: S) -> Result<Manifest> {
        let file = File::open(file)?;
        let r= bincode::deserialize_from(file);
        r.map_err(|e2| Error::new(ErrorKind::Other, e2))
    }
}

impl DirectoryEntry {
    pub fn empty<S: AsRef<OsStr>>(path: S) -> DirectoryEntry {
        DirectoryEntry {
            name: filename_to_string(Path::new(&path).file_name()),
            modification_time: SystemTime::now(),
            subdirs: Vec::new(),
            files: Vec::new(),
            hash_value: hash(&[]).unwrap(),
        }
    }

    pub fn new<S: AsRef<OsStr>>(path: S, config: &Configuration) -> Result<DirectoryEntry> {
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

fn hash(input: &[u8]) -> Result<ShaSum> {
    let mut sha256 = Context::new(&SHA256);
    let mut rv: ShaSum = [0u8; 32];
    sha256.update(input);
    sha256.finish().as_ref().read_exact(&mut rv)?;
    Ok(rv)
}

fn filename_to_string(filename: Option<&OsStr>) -> String {
    String::from(filename.unwrap().to_str().unwrap())
}