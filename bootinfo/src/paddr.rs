use core::fmt;

use bytemuck::Zeroable;

#[derive(Copy, Clone, Zeroable)]
#[repr(transparent)]
pub struct PAddr(pub usize);

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
