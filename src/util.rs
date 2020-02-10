
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

pub fn find_named_mut<T: Named, S: AsRef<str>>(all: &mut [T], name: S) -> Option<&mut T> {
    let name = name.as_ref();
    for candidate in all {
        if candidate.name() == name {
            return Some(candidate);
        }
    }
    None
}