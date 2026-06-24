//! Tables to be put inside the virtual memory register. Works with physical addresses; cannot be
//! accessed directly once virtualization isn't identity anymore.
//! See the `virt` module.

use bitflags::bitflags;

use super::assert_size_align;
use crate::PAGE_SIZE;

const _: () = assert_size_align::<Pml4Table>();
const _: () = assert_size_align::<PdptTable>();
const _: () = assert_size_align::<PdTable>();
const _: () = assert_size_align::<PtTable>();

pub const NODE_IS_LEAF: usize = 1 << 7;

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct Entry(usize);
bitflags! {
    impl Entry: usize {
        const PRESENT = 1 << 0;
        const READ_WRITE = 1 << 1;
        const USER_SUPERVISOR = 1 << 2;
        const WRITE_THROUGH = 1 << 3;
        const CACHE_DISABLE = 1 << 4;
        const ACCESSED = 1 << 5;

        const EXECUTE_DISABLE = 1 << 63;
    }
}

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct NodeEntry(usize);
impl NodeEntry {
    #[inline]
    pub const fn common(&self) -> &Entry {
        unsafe { &*(self as *const Self).cast() }
    }
}
bitflags! {
    impl NodeEntry: usize {
        const ADDRESS = (1 << 40 - 1) << 12;
    }
}

#[repr(C, align(4096))]
pub struct Pml4Table {
    pdpl_entries: [NodeEntry; PAGE_SIZE / size_of::<NodeEntry>()],
}

#[repr(C, align(4096))]
pub struct PdptTable {
    pdpl_entries: [PdptEntry; PAGE_SIZE / size_of::<PdptEntry>()],
}

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct PdptLeafEntry(usize);
impl PdptLeafEntry {
    #[inline]
    pub const fn common(&self) -> &Entry {
        unsafe { &*(self as *const Self).cast() }
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

#[derive(Copy, Clone)]
#[repr(C)]
pub union PdptEntry {
    pub node: NodeEntry,
    pub leaf: PdptLeafEntry,
}
impl PdptEntry {
    #[inline]
    pub const fn is_leaf(&self) -> bool {
        unsafe { (self as *const Self).cast::<usize>().read() & NODE_IS_LEAF == 1 }
    }
}

#[repr(C, align(4096))]
pub struct PdTable {
    pdpl_entries: [PdEntry; PAGE_SIZE / size_of::<PdEntry>()],
}

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct PdLeafEntry(usize);
impl PdLeafEntry {
    #[inline]
    pub const fn common(&self) -> &Entry {
        unsafe { &*(self as *const Self).cast() }
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

#[derive(Copy, Clone)]
#[repr(C)]
pub union PdEntry {
    pub node: NodeEntry,
    pub leaf: PdLeafEntry,
}
impl PdEntry {
    #[inline]
    pub const fn is_leaf(&self) -> bool {
        unsafe { (self as *const Self).cast::<usize>().read() & NODE_IS_LEAF == 1 }
    }
}

#[repr(C, align(4096))]
pub struct PtTable {
    pdpl_entries: [PtEntry; PAGE_SIZE / size_of::<PtEntry>()],
}

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct PtEntry(usize);
impl PtEntry {
    #[inline]
    pub const fn common(&self) -> &Entry {
        unsafe { &*(self as *const Self).cast() }
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
