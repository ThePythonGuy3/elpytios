use arrayvec::ArrayVec;
use elpytios_bootinfo::{PAGE_SIZE, paddr::PAddr};

use crate::alloc::AllocTree;

#[derive(Debug)]
pub struct PhysicalPageAllocator {
    trees: ArrayVec<Entry, { PAGE_SIZE / size_of::<Entry>() }>,
}

impl PhysicalPageAllocator {
    #[inline]
    pub const fn new() -> PhysicalPageAllocator {
        Self {
            trees: ArrayVec::new_const(),
        }
    }

    #[inline]
    pub const fn tree_count(&self) -> usize {
        self.trees.len()
    }

    #[inline]
    pub unsafe fn push_tree(&mut self, base: PAddr, tree: *mut AllocTree) {
        self.trees.push(Entry { base, tree });
    }
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    base: PAddr,
    tree: *mut AllocTree,
}
