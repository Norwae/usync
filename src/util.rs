use std::sync::mpsc::{Sender, Receiver};
use std::io::{Read, Error, Write, ErrorKind};
use std::cmp::min;

pub fn convert_error<E>(e: E) -> Error where E: Into<Box<dyn std::error::Error+Send+Sync>> {
    Error::new(ErrorKind::Other, e)
}

pub struct ReceiveAdapter {
    receiver: Receiver<Vec<u8>>,
    current: Vec<u8>,
    current_offset: usize
}

impl ReceiveAdapter {
    pub fn new(receiver: Receiver<Vec<u8>>) -> ReceiveAdapter {
        ReceiveAdapter { receiver, current: Vec::new(), current_offset: 0usize}
    }
}

impl Read for ReceiveAdapter {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        while self.current_offset == self.current.len() {
            self.current = self.receiver.recv().map_err(convert_error)?;
            self.current_offset = 0usize;
        }

        let current = &mut self.current;
        let remaining = &current.as_slice()[self.current_offset..];
        let take = min(buf.len(), remaining.len());
        let src = &remaining[..take];
        buf[..take].copy_from_slice(&src);
        self.current_offset += take;

        Ok(take)
    }
}

pub struct SendAdapter(Sender<Vec<u8>>);

impl SendAdapter {
    pub fn new(sender: Sender<Vec<u8>>) -> SendAdapter {
        SendAdapter(sender)
    }
}

impl Write for SendAdapter {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        const MAX_TRANSFERABLE_UNIT: usize = 16 << 20;
        let take = min(MAX_TRANSFERABLE_UNIT, buf.len());
        let v = Vec::from(&buf[..take]);
        self.0.send(v).map_err(convert_error)?;
        Ok(take)
    }

    fn flush(&mut self) -> Result<(), Error> {
        Ok(())
    }
}

pub trait Named {
    fn name(&self) -> &str;
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

