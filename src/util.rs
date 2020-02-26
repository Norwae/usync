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
            let next = self.receiver.recv();
            match next {
                Ok(next) => {
                    self.current = next;
                    self.current_offset = 0usize;
                },
                Err(_) => {
                    return Ok(0)
                },
            }
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

#[cfg(test)]
mod test_adapt {
    use super::*;
    use std::io::Error;
    use std::thread::JoinHandle;
    use std::io::Read;
    use std::sync::mpsc::channel;

    fn spawn_sut() -> (SendAdapter, JoinHandle<usize>){
        let (s, r) = channel();
        let sender = SendAdapter::new(s);
        let handle = std::thread::spawn(move || {
            let mut r = ReceiveAdapter::new(r);
            let mut consumed = 0usize;
            let mut got = 1usize;
            let mut buffer = [0u8;1024];
            while got > 0 {
                got = match r.read(&mut buffer) {
                    Ok(v) => v,
                    Err(e) => {
                        panic!(e);
                    },
                };
                consumed += got;
            }

            consumed
        });

        (sender, handle)
    }

    #[test]
    fn transfer_nothing() -> Result<(), Error> {
        let receive = {
            let (_, receive) = spawn_sut();
            receive
        };

        let receive = receive.join().unwrap();
        assert_eq!(receive, 0usize);
        Ok(())
    }

    #[test]
    fn transfer_some_bytes() -> Result<(), Error> {
        let receive = {
            let (mut send, receive) = spawn_sut();
            send.write(b"Hello World")?;
            receive
        };

        let receive = receive.join().unwrap();
        assert_eq!(receive, 11usize);
        Ok(())
    }

    #[test]
    fn transfer_huge_amount_of_bytes() -> Result<(), Error> {
        const N: usize = 65536;
        let receive = {
            let (mut send, receive) = spawn_sut();
            let mut v = Vec::with_capacity(N);
            for i in 0 ..= N {
                send.write(&v)?;
                v.push(i as u8)
            }
            receive
        };

        let receive = receive.join().unwrap();
        assert_eq!(receive, (N * (N + 1)) / 2);
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

#[cfg(test)]
mod test_named {
    use super::*;

    impl Named for &str {
        fn name(&self) -> &str {
            self
        }
    }

    #[test]
    fn find_successful() {
        assert_eq!(find_named(&["a", "b", "c", "d"], "c"), Some(&"c"))
    }

    #[test]
    fn find_unsuccessful() {
        assert_eq!(find_named(&["a", "b", "c", "d"], "q"), None)
    }
}