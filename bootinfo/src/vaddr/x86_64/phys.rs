//! Tables to be put inside the virtual memory register. Works with physical addresses; cannot be
//! accessed directly once virtualization isn't identity anymore.
//! See the `virt` module.

use core::mem;

use bitflags::bitflags;
use bytemuck::Zeroable;

use super::assert_size_align;
use crate::{PAGE_SIZE, paddr::PAddr};

const _: () = assert_size_align::<Pml4Table>();
const _: () = assert_size_align::<PdptTable>();
const _: () = assert_size_align::<PdTable>();
const _: () = assert_size_align::<PtTable>();

pub const NODE_IS_LEAF: usize = 1 << 7;

#[derive(Copy, Clone, Zeroable)]
#[repr(transparent)]
pub struct Entry(usize);
bitflags! {
    impl Entry: usize {
        /// 0 = unallocated, 1 = allocated
        const PRESENT = 1 << 0;
        /// 0 = read-only, 1 = write
        const READ_WRITE = 1 << 1;
        /// 0 = user-mode, 1 = kernel-mode
        const USER_SUPERVISOR = 1 << 2;
        const WRITE_THROUGH = 1 << 3;
        const CACHE_DISABLE = 1 << 4;
        const ACCESSED = 1 << 5;

        const EXECUTE_DISABLE = 1 << 63;
    }
}

#[derive(Debug, Copy, Clone, Zeroable)]
#[repr(transparent)]
pub struct NodeEntry(usize);
impl NodeEntry {
    #[inline]
    pub const fn from_common(entry: Entry) -> Self {
        Self(entry.0)
    }

    #[inline]
    pub const fn with_addr(self, addr: PAddr) -> Self {
        Self(self.0 & !Self::ADDRESS.0 | addr.0 & Self::ADDRESS.0)
    }

    #[inline]
    pub const fn addr(self) -> PAddr {
        PAddr(self.0 & Self::ADDRESS.0)
    }

    #[inline]
    pub const fn is_present(self) -> bool {
        self.0 & Entry::PRESENT.0 != 0
    }
}
bitflags! {
    impl NodeEntry: usize {
        const ADDRESS = (1 << 40 - 1) << 12;
    }
}

#[derive(Debug, Zeroable)]
#[repr(C, align(4096))]
pub struct Pml4Table {
    pub pdpt_entries: [NodeEntry; PAGE_SIZE / size_of::<NodeEntry>()],
}

#[derive(Zeroable)]
#[repr(C, align(4096))]
pub struct PdptTable {
    pub pd_entries: [PdptEntry; PAGE_SIZE / size_of::<PdptEntry>()],
}

#[derive(Copy, Clone, Zeroable)]
#[repr(transparent)]
pub struct PdptLeafEntry(usize);
impl PdptLeafEntry {
    #[inline]
    pub const fn from_common(entry: Entry) -> Self {
        Self(entry.0)
    }

    #[inline]
    pub const fn with_addr(self, addr: PAddr) -> Self {
        Self(self.0 & !Self::ADDRESS.0 | addr.0 & Self::ADDRESS.0)
    }

    #[inline]
    pub const fn addr(self) -> PAddr {
        PAddr(self.0 & Self::ADDRESS.0)
    }

    #[inline]
    pub const fn is_present(self) -> bool {
        self.0 & Entry::PRESENT.0 != 0
    }
}
bitflags! {
    impl PdptLeafEntry: usize {
        const GLOBAL = 1 << 8;
        const PAGE_ATTRIBUTE_TABLE = 1 << 12;

        const ADDRESS = (1 << 22 - 1) << 30;
        const PROTECTION_KEY = (1 << 4 - 1) << 59;
    }
}

#[derive(Copy, Clone, Zeroable)]
#[repr(C)]
pub union PdptEntry {
    node: NodeEntry,
    leaf: PdptLeafEntry,
}
impl PdptEntry {
    #[inline]
    pub const fn node(node: NodeEntry) -> Self {
        Self { node }
    }

    #[inline]
    pub const fn leaf(leaf: PdptLeafEntry) -> Self {
        unsafe { mem::transmute::<usize, Self>(mem::transmute::<Self, usize>(Self { leaf }) | NODE_IS_LEAF) }
    }

    #[inline]
    pub const fn is_leaf(self) -> bool {
        unsafe { mem::transmute::<Self, usize>(self) & NODE_IS_LEAF == 1 }
    }

    #[inline]
    pub const fn is_present(self) -> bool {
        unsafe { mem::transmute::<Self, usize>(self) & Entry::PRESENT.0 != 0 }
    }
}

#[derive(Zeroable)]
#[repr(C, align(4096))]
pub struct PdTable {
    pub pt_entries: [PdEntry; PAGE_SIZE / size_of::<PdEntry>()],
}

#[derive(Copy, Clone, Zeroable)]
#[repr(transparent)]
pub struct PdLeafEntry(usize);
impl PdLeafEntry {
    #[inline]
    pub const fn from_common(entry: Entry) -> Self {
        Self(entry.0)
    }

    #[inline]
    pub const fn with_addr(self, addr: PAddr) -> Self {
        Self(self.0 & !Self::ADDRESS.0 | addr.0 & Self::ADDRESS.0)
    }

    #[inline]
    pub const fn addr(self) -> PAddr {
        PAddr(self.0 & Self::ADDRESS.0)
    }

    #[inline]
    pub const fn is_present(self) -> bool {
        unsafe { mem::transmute::<Self, usize>(self) & Entry::PRESENT.0 != 0 }
    }
}
bitflags! {
    impl PdLeafEntry: usize {
        const GLOBAL = 1 << 8;
        const PAGE_ATTRIBUTE_TABLE = 1 << 12;

        const ADDRESS = (1 << 31 - 1) << 21;
        const PROTECTION_KEY = (1 << 4 - 1) << 59;
    }
}

#[derive(Copy, Clone, Zeroable)]
#[repr(C)]
pub union PdEntry {
    node: NodeEntry,
    leaf: PdLeafEntry,
}
impl PdEntry {
    #[inline]
    pub const fn node(node: NodeEntry) -> Self {
        Self { node }
    }

    #[inline]
    pub const fn leaf(leaf: PdLeafEntry) -> Self {
        unsafe { mem::transmute::<usize, Self>(mem::transmute::<Self, usize>(Self { leaf }) | NODE_IS_LEAF) }
    }

    #[inline]
    pub const fn is_leaf(self) -> bool {
        unsafe { mem::transmute::<Self, usize>(self) & NODE_IS_LEAF == 1 }
    }

    #[inline]
    pub const fn is_present(self) -> bool {
        unsafe { mem::transmute::<Self, usize>(self) & Entry::PRESENT.0 != 0 }
    }
}

#[derive(Zeroable)]
#[repr(C, align(4096))]
pub struct PtTable {
    pub phys_pages: [PtEntry; PAGE_SIZE / size_of::<PtEntry>()],
}

#[derive(Copy, Clone, Zeroable)]
#[repr(transparent)]
pub struct PtEntry(usize);
impl PtEntry {
    #[inline]
    pub const fn from_common(entry: Entry) -> Self {
        Self(entry.0)
    }

    #[inline]
    pub const fn with_addr(self, addr: PAddr) -> Self {
        Self(self.0 & !Self::ADDRESS.0 | addr.0 & Self::ADDRESS.0)
    }

    #[inline]
    pub const fn addr(self) -> PAddr {
        PAddr(self.0 & Self::ADDRESS.0)
    }

    #[inline]
    pub const fn is_present(self) -> bool {
        unsafe { mem::transmute::<Self, usize>(self) & Entry::PRESENT.0 != 0 }
    }
}
bitflags! {
    impl PtEntry: usize {
        const GLOBAL = 1 << 8;
        const PAGE_ATTRIBUTE_TABLE = 1 << 7;

        const ADDRESS = (1 << 40 - 1) << 12;
        const PROTECTION_KEY = (1 << 4 - 1) << 59;
    }
}
