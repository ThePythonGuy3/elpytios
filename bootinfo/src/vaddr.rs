use core::{
    fmt,
    ptr::{self, Pointee},
};

use bitflags::bitflags;
use bytemuck::Zeroable;

#[derive(Copy, Clone, Zeroable)]
#[repr(transparent)]
pub struct VAddr(usize);
impl VAddr {
    #[inline]
    pub const fn new(virtual_address: usize) -> Self {
        Self(virtual_address)
    }

    #[inline]
    pub const fn addr(self) -> usize {
        self.0
    }

    #[inline]
    pub const fn ptr<T: Pointee<Metadata = ()>>(self) -> *const T {
        ptr::from_raw_parts(ptr::with_exposed_provenance::<()>(self.0), ())
    }

    #[inline]
    pub const fn ptr_mut<T: Pointee<Metadata = ()>>(self) -> *mut T {
        ptr::from_raw_parts_mut(ptr::with_exposed_provenance_mut::<()>(self.0), ())
    }
}

bitflags! {
    impl VAddr: usize {
        const PAGE_OFFSET = ((1 << 12) - 1);
        const PT_INDEX = ((1 << 9) - 1) << 12;
        const PD_INDEX = ((1 << 9) - 1) << 21;
        const PDPT_INDEX = ((1 << 9) - 1) << 30;
        const PML4_INDEX = ((1 << 9) - 1) << 39;
    }
}

impl VAddr {
    #[inline]
    pub const fn indices(self) -> VAddrInfo {
        VAddrInfo {
            page_offset: self.0 & 0xfff,
            pt_index: (self.0 >> 12) & 0x1ff,
            pd_index: (self.0 >> 21) & 0x1ff,
            pdpt_index: (self.0 >> 30) & 0x1ff,
            pml4_index: (self.0 >> 39) & 0x1ff,
        }
    }
}

#[derive(Clone, Copy, Zeroable)]
#[repr(C)]
pub struct VAddrInfo {
    pub page_offset: usize,
    pub pt_index: usize,
    pub pd_index: usize,
    pub pdpt_index: usize,
    pub pml4_index: usize,
}

impl fmt::Debug for VAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl fmt::Display for VAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:p}", self.0 as *const ())
    }
}
