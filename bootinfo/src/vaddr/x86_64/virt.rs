use core::fmt;

use bitflags::bitflags;
use bytemuck::Zeroable;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Zeroable)]
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
    pub const fn ptr<T>(self) -> *const T {
        self.0 as *const T
    }

    #[inline]
    pub const fn ptr_mut<T>(self) -> *mut T {
        self.0 as *mut T
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

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct VirtualMap {
    recursion_index: usize,
}

impl VirtualMap {
    /// # Safety:
    /// `recursion_index` must be N where [`pdpt_entries[N]`](crate::vaddr::Pml4Table::pdpt_entries)
    /// points to the physical address of the PML4 table itself (i.e. recursive slot).
    #[inline]
    pub const unsafe fn new(recursion_index: usize) -> Self {
        Self { recursion_index }
    }
}
