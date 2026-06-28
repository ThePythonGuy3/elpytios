use core::ptr;

use elpytios_bootinfo::{PAGE_SIZE, paddr::PAddr};

use crate::alloc::AllocTree;

#[repr(C, align(4096))]
pub struct PhysicalPageAllocator {
    tree_count: usize,
    trees: [Entry; PAGE_SIZE / size_of::<Entry>() - 1],
}

impl PhysicalPageAllocator {
    #[inline]
    pub const fn new() -> PhysicalPageAllocator {
        Self {
            tree_count: 0,
            trees: [Entry {
                tree: ptr::from_raw_parts_mut(ptr::null_mut::<()>(), 0),
                base: PAddr::new(0),
            }; _],
        }
    }

    #[inline]
    pub const fn tree_count(&self) -> usize {
        self.tree_count
    }

    #[inline]
    pub const unsafe fn push_tree(&mut self, base: PAddr, tree: *mut AllocTree) {
        self.trees[self.tree_count] = Entry { base, tree };
        self.tree_count += 1;
    }
}

#[derive(Clone, Copy)]
struct Entry {
    base: PAddr,
    tree: *mut AllocTree,
}

const _: () = assert!(size_of::<PhysicalPageAllocator>() == PAGE_SIZE);
