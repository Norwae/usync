use crate::file_transfer::{FileAccess, command_handler_loop};
use std::path::{Path, PathBuf};
use std::fs::{Metadata, File};
use std::io::{Result, Read, Error, ErrorKind};
use std::sync::{Arc, Mutex};
use memmap::Mmap;
use std::cmp::min;
use std::collections::HashMap;
use std::net::TcpListener;
use crate::config::Configuration;
use crate::config::PathDefinition::Local;
use crate::tree::Manifest;
use std::thread;

pub struct Server {
    listener: TcpListener,
    root: PathBuf,
    manifest: Arc<Manifest>,
    verbose: bool
}

impl Server {
    pub fn run(&self) -> Result<()> {
        let registry = Arc::new(CachedFileRegistry::new());
        loop {
            let (conn, sa) = self.listener.accept()?;
            let root = self.root.clone();
            let manifest = self.manifest.clone();
            let registry = registry.clone();

            let verbose = self.verbose;
            if verbose {
                println!("Accepted connection {}", sa);
            }
            thread::spawn(move || {
                match command_handler_loop(&root, manifest.as_ref(), &conn, &conn, registry.as_ref()) {
                    Ok(_) => if verbose {
                        println!("Finished sending to {}", sa)
                    },
                    Err(err) => eprintln!("Command loop failed for {} with {}", sa, err),
                }
            });
        }
    }

    pub fn new(cfg: &Configuration) -> Result<Server> {
        if let Local(root) = cfg.source() {
            let root = root.to_owned();
            let verbose = cfg.verbose();
            let manifest = Arc::new(Manifest::create_persistent(&root, verbose, cfg.hash_settings(), cfg.manifest_path())?);
            let listener = TcpListener::bind(format!("0.0.0.0:{}", cfg.server_port()))?;

            Ok(Server{ listener, root, manifest, verbose})
        } else {
            Err(Error::new(ErrorKind::Other, "local path to serve from required"))
        }
    }
}

struct CachedFileEntry(Mmap, Metadata);

struct CachedFileRegistry {
    inner: Mutex<HashMap<PathBuf, Arc<CachedFileEntry>>>
}

struct ReadAdapter(Arc<CachedFileEntry>, usize);

impl Read for ReadAdapter {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let mapping = self.0.as_ref().0.as_ref();
        let mapping = &mapping[(self.1)..];
        let len = min(buf.len(), mapping.len());
        buf[..len].copy_from_slice(&mapping[..len]);
        self.1 += len;
        Ok(len)
    }
}

impl FileAccess for CachedFileRegistry {
    type Read = ReadAdapter;

    fn metadata(&self, path: &Path) -> Result<Metadata> {
        let mut inner = self.inner.lock().unwrap();
        match inner.get(path) {
            Some(v) => Ok(v.1.clone()),
            None => {
                let arc = Arc::new(CachedFileRegistry::new_entry(path)?);
                inner.insert(path.to_owned(), arc.clone());
                Ok(arc.1.clone())
            }
        }
    }

    fn read(&self, path: &Path) -> Result<Self::Read> {
        let mut inner = self.inner.lock().unwrap();
        match inner.get(path) {
            Some(v) => Ok(ReadAdapter(v.clone(), 0)),
            None => {
                let arc = Arc::new(CachedFileRegistry::new_entry(path)?);
                inner.insert(path.to_owned(), arc.clone());
                Ok(ReadAdapter(arc, 0))
            }
        }
    }
}

impl CachedFileRegistry {
    fn new() -> CachedFileRegistry {
        CachedFileRegistry {
            inner: Mutex::new(HashMap::new())
        }
    }

    fn new_entry(path: &Path) -> Result<CachedFileEntry> {
        let file = File::open(path)?;
        let map = unsafe { memmap::Mmap::map(&file)? };
        let meta = file.metadata()?;
        Ok(CachedFileEntry(map, meta))
    }
}
