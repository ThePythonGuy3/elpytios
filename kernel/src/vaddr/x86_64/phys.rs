use core::{fmt, mem};

use bitflags::bitflags;
use bytemuck::Zeroable;
use elpytios_bootinfo::{PAGE_SIZE, paddr::PAddr};

use super::assert_size_align;
use crate::vaddr::VFlags;

const _: () = assert_size_align::<Pml4Table>();
const _: () = assert_size_align::<PdptTable>();
const _: () = assert_size_align::<PdTable>();
const _: () = assert_size_align::<PtTable>();

pub const UNION_IS_LEAF: usize = 1 << 7;

#[derive(Debug, Copy, Clone, Zeroable)]
#[repr(transparent)]
pub struct Entry(usize);
bitflags! {
    impl Entry: usize {
        const PRESENT = 1 << 0;
        const WRITABLE = 1 << 1;
        const USER_MODE = 1 << 2;
        const WRITE_THROUGH = 1 << 3;
        const CACHE_DISABLED = 1 << 4;
        const ACCESSED = 1 << 5;

        const EXECUTE_DISABLE = 1 << 63;
    }
}

impl From<VFlags> for Entry {
    #[inline]
    fn from(value: VFlags) -> Self {
        let mut out = Self::empty();
        if value.contains(VFlags::WRITABLE) {
            out |= Self::WRITABLE
        }
        if value.contains(VFlags::USER_MODE) {
            out |= Self::USER_MODE
        }
        if value.contains(VFlags::WRITE_THROUGH) {
            out |= Self::WRITE_THROUGH
        }
        if value.contains(VFlags::CACHE_DISABLED) {
            out |= Self::CACHE_DISABLED
        }
        if value.contains(VFlags::ACCESSED) {
            out |= Self::ACCESSED
        }

        out
    }
}

#[derive(Debug, Clone, Copy, Zeroable)]
#[repr(transparent)]
pub struct NodeEntry(usize);
impl NodeEntry {
    /// # Safety:
    /// - `addr` must point to a **physical page** that is entirely contained by a valid child node.
    /// - Pointee at `addr` must be initialized and valid for accesses.
    #[inline]
    pub const unsafe fn new(entry: Entry, addr: PAddr) -> Self {
        Self((entry.0 | Entry::PRESENT.0) & !Self::ADDRESS_MASK.0 | addr.addr() & Self::ADDRESS_MASK.0)
    }

    #[inline]
    pub const fn child_addr(&self) -> PAddr {
        PAddr::new(self.0 & Self::ADDRESS_MASK.0)
    }

    #[inline]
    pub const fn is_present(&self) -> bool {
        self.0 & Entry::PRESENT.0 != 0
    }
}
bitflags! {
    impl NodeEntry: usize {
        const ADDRESS_MASK = ((1 << 40) - 1) << 12;
    }
}

#[derive(Debug, Zeroable)]
#[repr(C, align(4096))]
pub struct Pml4Table {
    pub pdpt_entries: [NodeEntry; PAGE_SIZE / size_of::<NodeEntry>()],
}

#[derive(Debug, Zeroable)]
#[repr(C, align(4096))]
pub struct PdptTable {
    pub pd_entries: [PdptEntry; PAGE_SIZE / size_of::<PdptEntry>()],
}

#[derive(Debug, Copy, Clone, Zeroable)]
#[repr(transparent)]
pub struct PdptLeafEntry(usize);
impl PdptLeafEntry {
    #[inline]
    pub const fn addr(self) -> PAddr {
        PAddr::new(self.0 & Self::ADDRESS_MASK.0)
    }

    /*#[inline]
    pub const fn new(entry: Entry, addr: PAddr) -> Self {
        Self((entry.0 | Entry::PRESENT.0) & !Self::ADDRESS_MASK.0 | addr.addr() & Self::ADDRESS_MASK.0)
    }

    #[inline]
    pub const fn is_present(self) -> bool {
        self.0 & Entry::PRESENT.0 != 0
    }*/
}
bitflags! {
    impl PdptLeafEntry: usize {
        const GLOBAL = 1 << 8;
        const PAGE_ATTRIBUTE_TABLE = 1 << 12;

        const ADDRESS_MASK = ((1 << 22) - 1) << 30;
        const PROTECTION_KEY = ((1 << 4) - 1) << 59;
    }
}

#[derive(Copy, Clone)]
pub enum UnionEntry<Node, Leaf> {
    Node(Node),
    Leaf(Leaf),
}

impl<Node, Leaf> UnionEntry<Node, Leaf> {
    #[inline]
    pub fn force_node(self) -> Node {
        let Self::Node(node) = self else { panic!("Not a node!") };
        node
    }

    /*#[inline]
    pub fn force_leaf(self) -> Leaf {
        let Self::Leaf(leaf) = self else { panic!("Not a node!") };
        leaf
    }*/
}

#[derive(Copy, Clone, Zeroable)]
#[repr(C)]
pub union PdptEntry {
    node: NodeEntry,
    leaf: PdptLeafEntry,
}
impl fmt::Debug for PdptEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind() {
            UnionEntry::Node(node) => write!(f, "PdptEntry({node:?})"),
            UnionEntry::Leaf(leaf) => write!(f, "PdptEntry({leaf:?})"),
        }
    }
}
impl PdptEntry {
    #[inline]
    pub const fn node(node: NodeEntry) -> Self {
        Self { node }
    }

    /*#[inline]
    pub const fn leaf(leaf: PdptLeafEntry) -> Self {
        unsafe { mem::transmute::<usize, Self>(mem::transmute::<Self, usize>(Self { leaf }) | NODE_IS_LEAF) }
    }*/

    #[inline]
    pub const fn kind(self) -> UnionEntry<NodeEntry, PdptLeafEntry> {
        unsafe {
            if mem::transmute::<Self, usize>(self) & UNION_IS_LEAF != 0 {
                UnionEntry::Leaf(self.leaf)
            } else {
                UnionEntry::Node(self.node)
            }
        }
    }

    /*#[inline]
    pub const fn kind_mut(&mut self) -> UnionEntry<&mut NodeEntry, &mut PdptLeafEntry> {
        unsafe {
            if mem::transmute::<Self, usize>(*self) & UNION_IS_LEAF != 0 {
                UnionEntry::Leaf(&mut self.leaf)
            } else {
                UnionEntry::Node(&mut self.node)
            }
        }
    }*/

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

#[derive(Debug, Copy, Clone, Zeroable)]
#[repr(transparent)]
pub struct PdLeafEntry(usize);
impl PdLeafEntry {
    #[inline]
    pub const fn addr(self) -> PAddr {
        PAddr::new(self.0 & Self::ADDRESS_MASK.0)
    }

    /*#[inline]
    pub const fn new(entry: Entry, addr: PAddr) -> Self {
        Self((entry.0 | Entry::PRESENT.0) & !Self::ADDRESS_MASK.0 | addr.addr() & Self::ADDRESS_MASK.0)
    }

    #[inline]
    pub const fn is_present(self) -> bool {
        unsafe { mem::transmute::<Self, usize>(self) & Entry::PRESENT.0 != 0 }
    }*/
}
bitflags! {
    impl PdLeafEntry: usize {
        const GLOBAL = 1 << 8;
        const PAGE_ATTRIBUTE_TABLE = 1 << 12;

        const ADDRESS_MASK = ((1 << 31) - 1) << 21;
        const PROTECTION_KEY = ((1 << 4) - 1) << 59;
    }
}

#[derive(Copy, Clone, Zeroable)]
#[repr(C)]
pub union PdEntry {
    node: NodeEntry,
    leaf: PdLeafEntry,
}
impl fmt::Debug for PdEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind() {
            UnionEntry::Node(node) => write!(f, "PdEntry({node:?})"),
            UnionEntry::Leaf(leaf) => write!(f, "PdEntry({leaf:?})"),
        }
    }
}
impl PdEntry {
    #[inline]
    pub const fn node(node: NodeEntry) -> Self {
        Self { node }
    }

    /*#[inline]
    pub const fn leaf(leaf: PdLeafEntry) -> Self {
        unsafe { mem::transmute::<usize, Self>(mem::transmute::<Self, usize>(Self { leaf }) | NODE_IS_LEAF) }
    }*/

    #[inline]
    pub const fn kind(self) -> UnionEntry<NodeEntry, PdLeafEntry> {
        unsafe {
            if mem::transmute::<Self, usize>(self) & UNION_IS_LEAF != 0 {
                UnionEntry::Leaf(self.leaf)
            } else {
                UnionEntry::Node(self.node)
            }
        }
    }

    /*#[inline]
    pub const fn kind_mut(&mut self) -> UnionEntry<&mut NodeEntry, &mut PdLeafEntry> {
        unsafe {
            if mem::transmute::<Self, usize>(*self) & NODE_IS_LEAF != 0 {
                UnionEntry::Leaf(&mut self.leaf)
            } else {
                UnionEntry::Node(&mut self.node)
            }
        }
    }*/

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

#[derive(Debug, Copy, Clone, Zeroable)]
#[repr(transparent)]
pub struct PtEntry(usize);
impl PtEntry {
    #[inline]
    pub const fn new(entry: Entry, addr: PAddr) -> Self {
        Self((entry.0 | Entry::PRESENT.0) & !Self::ADDRESS_MASK.0 | addr.addr() & Self::ADDRESS_MASK.0)
    }

    #[inline]
    pub const fn addr(self) -> PAddr {
        PAddr::new(self.0 & Self::ADDRESS_MASK.0)
    }

    #[inline]
    pub const fn is_present(self) -> bool {
        self.0 & Entry::PRESENT.0 != 0
    }
}
bitflags! {
    impl PtEntry: usize {
        const GLOBAL = 1 << 8;
        const PAGE_ATTRIBUTE_TABLE = 1 << 7;

        const ADDRESS_MASK = ((1 << 40) - 1) << 12;
        const PROTECTION_KEY = ((1 << 4) - 1) << 59;
    }
}
