use core::{
    fmt,
    ptr::{self, Pointee},
};

use bytemuck::Zeroable;

#[derive(Copy, Clone, Zeroable)]
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
    pub const fn identity<T: Pointee<Metadata = ()>>(self) -> *const T {
        ptr::from_raw_parts(ptr::with_exposed_provenance::<()>(self.0), ())
    }

    #[inline]
    pub const fn identity_mut<T: Pointee<Metadata = ()>>(self) -> *mut T {
        ptr::from_raw_parts_mut(ptr::with_exposed_provenance_mut::<()>(self.0), ())
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
