use std::ffi::OsStr;
use std::fs::{File, read_dir};
use std::io::{Error, ErrorKind, Read, Result, BufReader, BufWriter, empty};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use ring::digest::{Context, SHA256};
use serde::{Serialize, Deserialize};

use crate::config::{ManifestMode, HashSettings};
use crate::util::{Named, find_named};
use crate::file_transfer::Transmitter;

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
    fn new(path: &Path, verbose: bool, settings: &HashSettings) -> Result<FileEntry> {

        let hash_value = if settings.manifest_mode() == ManifestMode::Hash {
            hash(File::open(path)?)?
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
    fn validate0(&self, path: &mut PathBuf, settings: &HashSettings) -> Result<bool> {
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
            path.push(&name);

            if settings.is_excluded(path.as_ref()) {
                path.pop();
                continue;
            }

            examined_count += 1;
            if entry.metadata()?.is_dir() {
                let found = find_named(self.subdirs.as_slice(), name.to_string_lossy());
                match found {
                    None => return Ok(false),
                    Some(o) => {
                        if !o.validate0(path, settings)? {
                            return Ok(false);
                        }
                        path.pop();
                    }
                }
            } else {
                let found = find_named(self.files.as_slice(), &name.to_string_lossy());
                match found {
                    None => return Ok(false),
                    Some(o) => {
                        let meta = path.metadata()?;
                        let mismatch =
                            meta.modified()? != o.modification_time ||
                                meta.len() != o.file_size;
                        if mismatch {
                            return Ok(false);
                        }
                        path.pop();
                    }
                }
            }
        }
        let count_match = examined_count == (self.subdirs.len() + self.files.len());

        Ok(count_match)
    }


    fn validate(&self, path: &mut PathBuf, settings: &HashSettings) -> bool {
        self.validate0(path, settings).unwrap_or(false)
    }

    fn copy_from<T: Transmitter>(&self, path: &Path, source: &DirectoryEntry, transmitter: &mut T, verbose: bool)-> Result<()> {
        self.copy_subdirs(path, &source, transmitter, verbose)?;
        self.copy_files(path, &source, transmitter, verbose)?;
        Ok(())
    }

    fn copy_files<T: Transmitter>(&self, path: &Path, source: &DirectoryEntry, transmitter: &mut T, verbose: bool) -> Result<()>{
        for source_file in &source.files {
            let existing_file = find_named(self.files.as_slice(), &source_file.name);
            let this_path = path.join(&source_file.name);

            match existing_file {
                None => {
                    if verbose {
                        println!("Transmitting new file: {}", &this_path.to_string_lossy())
                    }
                    transmitter.transmit(&this_path)?
                }
                Some(existing) => {
                    if existing != source_file {
                        if verbose {
                            println!("Overwriting changed file: {}", &this_path.to_string_lossy());
                        }
                        transmitter.transmit(&this_path)?
                    }
                }
            }
        }

        Ok(())
    }

    fn copy_subdirs<T: Transmitter>(&self, path: &Path, source: &DirectoryEntry, transmitter: &mut T, verbose: bool) -> Result<()>{
        for source_dir in &source.subdirs {
            let existing_subdir = find_named(self.subdirs.as_slice(), &source_dir.name);
            let this_path = path.join(&source_dir.name);

            match existing_subdir {
                None => {
                    let subdir = DirectoryEntry::empty(&source_dir.name);
                    subdir.copy_from(&this_path, source_dir, transmitter, verbose)?;
                }
                Some(existing) => {
                    if existing != source_dir {
                        existing.copy_from(&this_path, source_dir, transmitter, verbose)?;
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
            hash_value: hash(empty()).unwrap(),
        }
    }

    pub fn new<S: AsRef<OsStr>>(path: S, verbose: bool, settings: &HashSettings) -> Result<DirectoryEntry> {
        DirectoryEntry::create(&mut PathBuf::from(path.as_ref()), verbose, settings)
    }

    fn create(pb: &mut PathBuf, verbose: bool, settings: &HashSettings) -> Result<DirectoryEntry> {
        let dir = read_dir(&pb)?;
        let mut subdirs: Vec<DirectoryEntry> = Vec::new();
        let mut files: Vec<FileEntry> = Vec::new();
        let mut hash_input: Vec<u8> = Vec::new();
        let modification_time = pb.metadata()?.modified()?;
        let name = filename_to_string(pb.file_name());

        for entry in dir {
            let entry = entry?;
            pb.push(entry.file_name());

            if settings.is_excluded(pb.as_ref()) {
                if verbose {
                    println!("Excluding file {}", pb.to_string_lossy())
                }
                pb.pop();
                continue;
            }

            if entry.metadata()?.is_dir() {
                let subtree = DirectoryEntry::create(pb, verbose, settings)?;
                hash_input.extend(subtree.name.as_bytes());
                hash_input.extend(&subtree.hash_value);
                subdirs.push(subtree);
            } else {
                let file = FileEntry::new(pb, verbose, settings)?;
                hash_input.extend(file.name.as_bytes());
                hash_input.extend(&file.file_size.to_le_bytes());
                hash_input.extend(&file.hash_value);
                files.push(file);
            }

            pb.pop();
        }

        let hash_value = hash(hash_input.as_slice())?;

        if verbose {
            println!("Hashed directory {} into {}", pb.to_string_lossy(), hex::encode(&hash_value))
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
    pub fn create_ephemeral<S: AsRef<OsStr>>(root: S, verbose: bool, settings: &HashSettings) -> Result<Manifest> {
        let de = DirectoryEntry::new(root.as_ref(), verbose, settings)?;

        Ok(Manifest(de))
    }

    pub fn create_persistent<S: AsRef<OsStr>>(root: S, verbose: bool, settings: &HashSettings, manifest_path: &Path) -> Result<Manifest> {
        let manifest_path = manifest_file(root.as_ref(), manifest_path);
        let settings = settings.with_additional_exclusion(manifest_path.as_path());

        if verbose {
            println!("Resolved manifest path to {}", manifest_path.as_path().to_string_lossy());
        }

        let mut res = Manifest::load(manifest_path.as_path(), &settings);
        if res.is_ok() {
            let m = res.as_ref().unwrap();
            if !m.0.validate(&mut PathBuf::from(root.as_ref()), &settings) {
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

                manifest.save(verbose, &manifest_path)?;

                Ok(manifest)
            })
        })
    }

    pub fn copy_from<T: Transmitter>(&self, source: &Manifest, transmitter: &mut T, verbose: bool) -> Result<()> {
        let path = PathBuf::new();
        let source = &source.0;
        self.0.copy_from(&path, source, transmitter, verbose)?;

        Ok(())
    }

    fn save(&self, verbose: bool, manifest_path: &Path) -> Result<()> {
        if verbose {
            println!("Opening file {} for saving manifest", manifest_path.to_string_lossy());
        }

        let file = File::create(manifest_path)?;
        let r = bincode::serialize_into(BufWriter::new(file), self);
        r.map_err(|e| Error::new(ErrorKind::Other, e))?;

        if verbose {
            println!("Saved manifest file to {}", manifest_path.to_string_lossy());
        }

        Ok(())
    }

    fn load<S: AsRef<Path>>(file: S, cfg: &HashSettings) -> Result<Manifest> {
        if cfg.force_rebuild() {
            return Err(Error::new(ErrorKind::Other, "Forced rebuild of manifest"));
        }
        let file = File::open(file)?;
        bincode::deserialize_from(BufReader::new(file))
            .map_err(|e| Error::new(ErrorKind::Other, e))
    }
}


fn hash<R: Read>(mut input: R) -> Result<ShaSum> {
    let mut sha256 = Context::new(&SHA256);
    let mut rv: ShaSum = [0u8; 32];
    let mut buffer = [0u8; 65536];
    let mut received = input.read(&mut buffer)?;
    while received != 0 {
        sha256.update(&buffer[..received]);
        received = input.read(&mut buffer)?;
    }
    sha256.finish().as_ref().read_exact(&mut rv)?;
    Ok(rv)
}

fn filename_to_string(filename: Option<&OsStr>) -> String {
    String::from(filename.unwrap().to_string_lossy())
}

fn manifest_file(root: &OsStr, cfg_path: &Path) -> PathBuf {
    let mut manifest_path = PathBuf::new();

    if cfg_path.is_absolute() {
        manifest_path.push(cfg_path);
    } else {
        manifest_path.push(root);
        manifest_path.push(cfg_path);
    }

    manifest_path
}