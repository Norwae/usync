use std::time::SystemTime;
use std::fs::File;
use std::io::{Error, Read, Write};

use ring::digest::{SHA256, Context, Digest};

type ShaSum = [u8;32];

#[derive(Debug)]
pub struct FileEntry {
    name: String,
    modification_time: SystemTime,
    pub(crate) hash_value: ShaSum
}

impl FileEntry {
    pub fn new(path: &str) -> Result<FileEntry, Error> {
        let mut file = File::open(path)?;
        let mod_time = file.metadata()?.modified()?;
        let hashed = hash(&mut file)?;

        Ok(FileEntry{
            name: String::from(path),
            modification_time: mod_time,
            hash_value: hashed
        })
    }

}

fn hash<R: Read>(input: &mut R)-> Result<ShaSum, Error> {
    let mut sha256 = Context::new(&SHA256);
    let mut buffer = [0u8; 65536];
    let mut rv = [0u8;32];

    loop {
        let got = input.read(&mut buffer)?;
        if got == 0 {
            break;
        }
        sha256.update(&buffer[0..got]);
    }

    sha256.finish().as_ref().read_exact(&mut rv);
    Ok(rv)
}