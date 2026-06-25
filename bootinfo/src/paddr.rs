use core::fmt;

use bytemuck::Zeroable;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Zeroable)]
#[repr(transparent)]
pub struct PAddr(usize);
impl PAddr {
    #[inline]
    pub const fn new(addr: usize) -> Self {
        Self(addr)
    }

    #[inline]
    pub const fn addr(self) -> usize {
        self.0
    }

    #[inline]
    pub const fn byte_add(self, offset: usize) -> Self {
        Self(self.0 + offset)
    }
}

impl fmt::Debug for PAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl fmt::Display for PAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:p}", self.0 as *const ())
    }
}
