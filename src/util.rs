use std::sync::mpsc;
use std::sync::mpsc::{Sender, Receiver};
use std::io::{Read, Error, Write, ErrorKind};
use std::cmp::min;

pub trait Named {
    fn name(&self) -> &str;
}

pub struct ReceiveAdapter {
    receiver: Receiver<Vec<u8>>,
    current: Vec<u8>
}

impl ReceiveAdapter {
    pub fn new(receiver: Receiver<Vec<u8>>) -> ReceiveAdapter {
        ReceiveAdapter { receiver, current: Vec::new()}
    }
}

impl Read for ReceiveAdapter {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        while self.current.is_empty() {
            self.current = self.receiver.recv().map_err(convert_error)?
        }

        let current = &mut self.current;
        let take = min(buf.len(), current.len());
        let src = &current.as_slice()[..take];
        buf[..take].copy_from_slice(&src);
        current.as_mut_slice().copy_within((take..), 0);
        current.truncate(current.len() - take);

        Ok(take)
    }
}


pub fn convert_error<E>(e: E) -> Error where E: Into<Box<dyn std::error::Error+Send+Sync>> {
    Error::new(ErrorKind::Other, e)
}

pub struct SendAdapter(Sender<Vec<u8>>);

impl SendAdapter {
    pub fn new(sender: Sender<Vec<u8>>) -> SendAdapter {
        SendAdapter(sender)
    }
}

impl Write for SendAdapter {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        let v = Vec::from(buf);
        self.0.send(v).map_err(convert_error)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

pub fn find_named<T: Named, S : AsRef<str>>(all: &[T], name: S) -> Option<&T> {
    let name = name.as_ref();
    for candidate in all {
        if candidate.name() == name {
            return Some(candidate);
        }
    }
    None
}

