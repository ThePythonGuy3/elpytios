use core::fmt;

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct PAddr {
    addr: usize,
}

impl fmt::Debug for PAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl fmt::Display for PAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:p}", self.addr as *const ())
    }
}
