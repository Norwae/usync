use std::ffi::OsStr;
use std::fs::{File, read_dir};
use std::io::{Error, ErrorKind, Read, Result};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use ring::digest::{Context, SHA256};
use serde::{Serialize, Deserialize};

use crate::config::{ManifestMode, Configuration, HashSettings};
use crate::util::{Named, find_named};
use crate::file_transfer::Transmitter;
use std::collections::HashSet;

type ShaSum = [u8; 32];

#[derive(Debug, Serialize, Deserialize)]
struct FileEntry {
    name: String,
    modification_time: SystemTime,
    file_size: u64,
    hash_value: ShaSum,
}

impl PartialEq for FileEntry {
    fn eq(&self, other: &Self) -> bool {
        self.file_size == other.file_size &&
            self.modification_time == other.modification_time &&
            self.hash_value == other.hash_value
    }
}

impl Named for FileEntry {
    fn name(&self) -> &str {
        &self.name
    }
}

impl FileEntry {
    pub fn new<S: AsRef<OsStr>>(path: S, verbose: bool, settings: &HashSettings) -> Result<FileEntry> {
        let path = PathBuf::from(&path);

        let hash_value = if settings.manifest_mode() == ManifestMode::Hash {
            unsafe {
                let file = File::open(path.as_path())?;
                let mmap = memmap2::Mmap::map(&file)?;
                hash(mmap.as_ref())?
            }
        } else {
            [0u8; 32]
        };

        let metadata = path.metadata()?;
        let name = filename_to_string(path.file_name());

        if verbose {
            println!("Hashed file {} into {}", path.to_string_lossy(), hex::encode(&hash_value))
        }

        Ok(FileEntry {
            name,
            modification_time: metadata.modified()?,
            file_size: metadata.len(),
            hash_value,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct DirectoryEntry {
    name: String,
    modification_time: SystemTime,
    subdirs: Vec<DirectoryEntry>,
    files: Vec<FileEntry>,
    hash_value: ShaSum,
}

impl DirectoryEntry {
    fn validate0(&self, path: &Path, settings: &HashSettings) -> Result<bool> {
        if !path.exists() {
            return Ok(false);
        }

        let meta = path.metadata()?;
        let mtime = meta.modified()?;

        if !meta.is_dir() || mtime != self.modification_time {
            return Ok(false);
        }

        let mut examined_count = 0usize;
        for entry in path.read_dir()? {
            let entry = entry?;
            let name = entry.file_name();
            let sub_path = path.join(&name);

            if settings.is_excluded(sub_path.as_path()) {
                continue;
            }

            examined_count += 1;
            if entry.metadata()?.is_dir() {
                let found = find_named(self.subdirs.as_slice(), name.to_string_lossy());
                match found {
                    None => return Ok(false),
                    Some(o) => {
                        if !o.validate(sub_path, settings) {
                            return Ok(false);
                        }
                    }
                }
            } else {
                let found = find_named(self.files.as_slice(), &name.to_string_lossy());
                match found {
                    None => return Ok(false),
                    Some(o) => {
                        let meta = sub_path.metadata()?;
                        let mismatch =
                            meta.modified()? != o.modification_time ||
                                meta.len() != o.file_size;
                        if mismatch {
                            return Ok(false);
                        }
                    }
                }
            }
        }
        let count_match = examined_count == (self.subdirs.len() + self.files.len());

        Ok(count_match)
    }


    fn validate<S: AsRef<OsStr>>(&self, path: S, settings: &HashSettings) -> bool {
        self.validate0(Path::new(path.as_ref()), settings).unwrap_or(false)
    }

    fn copy_from(&self, path: &Path, source: &DirectoryEntry, transmitter: &dyn Transmitter, cfg: &Configuration)-> Result<()> {
        self.copy_subdirs(path, &source, transmitter, cfg)?;
        self.copy_files(path, &source, transmitter, cfg)?;
        Ok(())
    }

    fn copy_files(&self, path: &Path, source: &DirectoryEntry, transmitter: &dyn Transmitter, cfg: &Configuration) -> Result<()>{
        for source_file in &source.files {
            let existing_file = find_named(self.files.as_slice(), &source_file.name);
            let this_path = path.join(&source_file.name);

            match existing_file {
                None => {
                    if cfg.verbose() {
                        println!("Transmitting new file: {}", &this_path.to_string_lossy())
                    }
                    transmitter.transmit(&this_path)?
                }
                Some(existing) => {
                    if existing != source_file {
                        if cfg.verbose() {
                            println!("Overwriting changed file: {}", &this_path.to_string_lossy());
                        }
                        transmitter.transmit(&this_path)?
                    }
                }
            }
        }

        Ok(())
    }

    fn copy_subdirs(&self, path: &Path, source: &&DirectoryEntry, transmitter: &dyn Transmitter, cfg: &Configuration) -> Result<()>{
        for source_dir in &source.subdirs {
            let existing_subdir = find_named(self.subdirs.as_slice(), &source_dir.name);
            let this_path = path.join(&source_dir.name);

            match existing_subdir {
                None => {
                    let subdir = DirectoryEntry::empty(&source_dir.name);
                    subdir.copy_from(&this_path, source_dir, transmitter, cfg)?;
                }
                Some(existing) => {
                    if existing != source_dir {
                        existing.copy_from(&this_path, source_dir, transmitter, cfg)?;
                    }
                }
            }
        }

        Ok(())
    }

    fn empty(name: &str) -> DirectoryEntry {
        DirectoryEntry {
            name: String::from(name),
            modification_time: SystemTime::now(),
            subdirs: Vec::new(),
            files: Vec::new(),
            hash_value: hash(&[]).unwrap(),
        }
    }

    pub fn new<S: AsRef<OsStr>>(path: S, verbose: bool, settings: &HashSettings) -> Result<DirectoryEntry> {
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

            if settings.is_excluded(sub_path.as_path()) {
                if verbose {
                    println!("Excluding file {}", sub_path.to_string_lossy())
                }
                continue;
            }

            if entry.metadata()?.is_dir() {
                let subtree = DirectoryEntry::new(sub_path, verbose, settings)?;
                hash_input.extend(subtree.name.as_bytes());
                hash_input.extend(&subtree.hash_value);
                subdirs.push(subtree);
            } else {
                let file = FileEntry::new(sub_path, verbose, settings)?;
                hash_input.extend(file.name.as_bytes());
                hash_input.extend(&file.file_size.to_le_bytes());
                hash_input.extend(&file.hash_value);
                files.push(file);
            }
        }

        let hash_value = hash(hash_input.as_ref())?;

        if verbose {
            println!("Hashed directory {} into {}", path.to_string_lossy(), hex::encode(&hash_value))
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

impl PartialEq for DirectoryEntry {
    fn eq(&self, other: &Self) -> bool {
        other.modification_time == self.modification_time &&
            other.hash_value == self.hash_value
    }
}

impl Named for DirectoryEntry {
    fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Serialize, Deserialize)]
pub struct Manifest(DirectoryEntry);

impl Manifest {
    pub fn create_ephemeral<S: AsRef<OsStr>>(root: S, cfg: &Configuration) -> Result<Manifest> {
        let de = DirectoryEntry::new(root.as_ref(), cfg.verbose(),cfg.hash_settings())?;

        Ok(Manifest(de))
    }

    pub fn create_persistent<S: AsRef<OsStr>>(root: S, cfg: &Configuration) -> Result<Manifest> {
        let manifest_path = manifest_file(root.as_ref(), &cfg);
        let settings = cfg.hash_settings().with_additional_exclusion(manifest_path.as_path());
        let verbose = cfg.verbose();

        if verbose {
            println!("Resolved manifest path to {}", manifest_path.as_path().to_string_lossy());
        }

        let mut res = Manifest::_load(manifest_path.as_path(), &settings);
        if res.is_ok() {
            let m = res.as_ref().unwrap();
            if !m.0.validate(root.as_ref(), &settings) {
                res = Err(Error::new(ErrorKind::Other, "Manifest validation failed"))
            }
        }

        res.or_else(|e| {
            if verbose {
                println!("Manifest file not usable: {}", e)
            }
            let de = DirectoryEntry::new(root.as_ref(), verbose, &settings);
            de.and_then(|e| {
                let manifest = Manifest(e);

                manifest.save(root.as_ref(), &cfg)?;

                Ok(manifest)
            })
        })
    }

    pub fn copy_from(&self, source: &Manifest, transmitter: &dyn Transmitter, cfg: &Configuration) -> Result<()> {
        let path = PathBuf::new();
        let source = &source.0;
        self.0.copy_from(&path, source, transmitter, cfg)?;

        Ok(())
    }

    pub fn save<S: AsRef<OsStr>>(&self, root: S, cfg: &Configuration) -> Result<()> {
        let manifest_path = manifest_file(root.as_ref(), cfg);
        println!("Opening file {} for saving manifest", manifest_path.to_string_lossy());
        let file = File::create(manifest_path.as_path())?;
        let r = bincode::serialize_into(file, self);
        r.map_err(|e2| Error::new(ErrorKind::Other, e2))?;

        if cfg.verbose() {
            println!("Saved manifest file to {}", manifest_path.to_string_lossy());
        }

        Ok(())
    }

    fn _load<S: AsRef<Path>>(file: S, cfg: &HashSettings) -> Result<Manifest> {
        if cfg.force_rebuild() {
            return Err(Error::new(ErrorKind::Other, "Forced rebuild of manifest"));
        }
        let file = File::open(file)?;
        bincode::deserialize_from(file)
            .map_err(|e2| Error::new(ErrorKind::Other, e2))
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
    String::from(filename.unwrap().to_string_lossy())
}

fn manifest_file(root: &OsStr, cfg: &Configuration) -> PathBuf {
    let mut manifest_path = PathBuf::new();
    let cfg_path = cfg.manifest_path();
    if cfg_path.is_absolute() {
        manifest_path.push(cfg_path);
    } else {
        manifest_path.push(root);
        manifest_path.push(cfg_path);
    }

    manifest_path
}