use core::sync::atomic::{AtomicBool, Ordering::Relaxed};

use arrayvec::ArrayVec;
use elpytios_bootinfo::{PAGE_SIZE, paddr::PAddr};

use crate::allocator::{AllocTree, TreeAllocError};

#[derive(Debug)]
pub struct PhysicalPageAllocator {
    trees: ArrayVec<Entry, { PAGE_SIZE / size_of::<Entry>() }>,
}

impl PhysicalPageAllocator {
    #[inline]
    pub fn new() -> PhysicalPageAllocator {
        static CREATED: AtomicBool = AtomicBool::new(false);

        if CREATED.swap(true, Relaxed) {
            panic!("Only one `PhysicalPageAllocator` instance may be created")
        }

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

    pub fn alloc(&mut self, order: u32) -> Result<PAddr, TreeAllocError> {
        let mut last_error = TreeAllocError::InsufficientSpace { requested_order: order };
        for &Entry { base, tree } in &self.trees {
            let tree = unsafe { tree.as_mut_unchecked() };
            match tree.alloc(order) {
                Ok(index) => return Ok(base.byte_add(index as usize * PAGE_SIZE)),
                Err(e @ TreeAllocError::Zero) => return Err(e),
                Err(e @ TreeAllocError::InsufficientSpace { .. }) => last_error = e,
            }
        }

        Err(last_error)
    }
}

unsafe impl Sync for PhysicalPageAllocator {}

#[derive(Debug, Clone, Copy)]
struct Entry {
    base: PAddr,
    tree: *mut AllocTree,
}
