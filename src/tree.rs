use std::ffi::OsStr;
use std::fs::{File, read_dir, create_dir};
use std::io::{Error, ErrorKind, Read, Result};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use ring::digest::{Context, SHA256};
use serde::{Serialize, Deserialize};

use crate::config::{ManifestMode, Configuration};
use crate::util::{Named, find_named};

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
    pub fn empty(path: &Path) -> FileEntry {
        FileEntry {
            name: String::from(path.to_path_buf().file_name().unwrap().to_string_lossy()),
            modification_time: SystemTime::now(),
            file_size: 0,
            hash_value: [0u8; 32]
        }
    }

    pub fn copy(target: &Path, src: &Path) -> Result<()>{
        let metadata = src.metadata()?;
        let time = filetime::FileTime::from_last_modification_time(&metadata);
        std::fs::copy(src, target)?;
        filetime::set_file_mtime(target, time)?;
        Ok(())
    }

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
        let name = filename_to_string(path.file_name());

        if config.verbose {
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
    fn validate0(&self, path: &Path, cfg: &Configuration) -> Result<bool> {
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

            if cfg.is_excluded(sub_path.as_path()) {
                continue;
            }

            examined_count += 1;
            if entry.metadata()?.is_dir() {
                let found = find_named(self.subdirs.as_slice(), name.to_string_lossy());
                match found {
                    None => return Ok(false),
                    Some(o) => {
                        if !o.validate(sub_path, cfg) {
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


    fn validate<S: AsRef<OsStr>>(&self, path: S, cfg: &Configuration) -> bool {
        self.validate0(Path::new(path.as_ref()), cfg).unwrap_or(false)
    }

    fn copy_from(&self, target_path: &Path, source: &DirectoryEntry, source_path: &Path, cfg: &Configuration)-> Result<()> {
        for source_dir in  &source.subdirs {
            let existing_subdir = find_named(self.subdirs.as_slice(), &source_dir.name);
            let mut source_path = source_path.to_path_buf();
            source_path.push(&source_dir.name);
            let mut target_path = target_path.to_path_buf();
            target_path.push(&source_dir.name);

            match existing_subdir {
                None => {
                    let subdir = DirectoryEntry::empty(&target_path);
                    create_dir(&target_path)?;
                    subdir.copy_from(target_path.as_path(), source_dir,source_path.as_path(), cfg)?;
                }
                Some(existing) => {
                    if existing != source_dir {
                        existing.copy_from(target_path.as_path(), source_dir, source_path.as_path(), cfg)?;
                    }
                }
            }
        }

        for source_file in &source.files {
            let existing_file = find_named(self.files.as_slice(), &source_file.name);
            let mut source_path = source_path.to_path_buf();
            source_path.push(&source_file.name);
            let mut target_path = target_path.to_path_buf();
            target_path.push(&source_file.name);

            match existing_file {
                None => FileEntry::copy(target_path.as_ref(), source_path.as_ref())?,
                Some(existing) => {
                    if existing != source_file {
                        FileEntry::copy(target_path.as_ref(), source_path.as_ref())?
                    }
                }
            }
        }
        Ok(())
    }

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

            if config.is_excluded(sub_path.as_path()) {
                if config.verbose {
                    println!("Excluding file {}", sub_path.to_string_lossy())
                }
                continue;
            }

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
pub struct Manifest(DirectoryEntry, PathBuf);

impl Manifest {
    pub fn create_ephemeral<S: AsRef<OsStr>>(root: S, cfg: &Configuration) -> Result<Manifest> {
        let de = DirectoryEntry::new(root.as_ref(), cfg)?;

        Ok(Manifest(de, PathBuf::from(root.as_ref())))
    }

    pub fn create_persistent<S: AsRef<OsStr>>(root: S, cfg: &Configuration) -> Result<Manifest> {
        let manifest_path = manifest_file(root.as_ref(), &cfg);
        let mut cfg = cfg.with_additional_exclusion(manifest_path.as_path());

        if cfg.verbose {
            println!("Resolved manifest path to {}", manifest_path.as_path().to_string_lossy());
        }

        let mut res = Manifest::_load(manifest_path.as_path(), &cfg);
        if res.is_ok() {
            let m = res.as_ref().unwrap();
            if !m.0.validate(root.as_ref(), &cfg) {
                res = Err(Error::new(ErrorKind::Other, "Manifest validation failed"))
            }
        }

        res.or_else(|e| {
            if cfg.verbose {
                println!("Manifest file not usable: {}", e)
            }
            let de = DirectoryEntry::new(root.as_ref(), &cfg);
            de.and_then(|e| {
                let manifest = Manifest(e, PathBuf::from(root.as_ref()));

                manifest.save(root.as_ref(), &cfg)?;

                Ok(manifest)
            })
        })
    }

    pub fn copy_from(&self, source: &Manifest, cfg: &Configuration) -> Result<()> {
        self.0.copy_from(self.1.as_path(),&source.0, source.1.as_path(), cfg)?;

        Ok(())
    }

    pub fn save<S: AsRef<OsStr>>(&self, root: S, cfg: &Configuration) -> Result<()> {
        let manifest_path = manifest_file(root.as_ref(), cfg);
        println!("Opening file {} for saving manifest", manifest_path.to_string_lossy());
        let file = File::create(manifest_path.as_path())?;
        let r = bincode::serialize_into(file, self);
        r.map_err(|e2| Error::new(ErrorKind::Other, e2))?;

        if cfg.verbose {
            println!("Saved manifest file to {}", manifest_path.to_string_lossy());
        }

        Ok(())
    }

    fn _load<S: AsRef<Path>>(file: S, cfg: &Configuration) -> Result<Manifest> {
        if cfg.force_rebuild_manifest {
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
    if cfg.manifest_path.is_absolute() {
        manifest_path.push(&cfg.manifest_path);
    } else {
        manifest_path.push(root);
        manifest_path.push(&cfg.manifest_path);
    }

    manifest_path
}