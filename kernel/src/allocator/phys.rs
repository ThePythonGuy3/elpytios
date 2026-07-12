use core::{
    hint::cold_path,
    sync::atomic::{AtomicBool, Ordering::Relaxed},
};

use arrayvec::ArrayVec;
use elpytios_bootinfo::{PAGE_SIZE, paddr::PAddr};

use crate::allocator::{AllocTree, TreeAllocError};

#[derive(Debug)]
pub struct PhysicalPageAllocator {
    trees: ArrayVec<Entry, { PAGE_SIZE / size_of::<Entry>() }>,
}

impl PhysicalPageAllocator {
    /// # Safety
    /// [`Self::sort_tree`] must be called before allocating.
    #[inline]
    pub unsafe fn new() -> PhysicalPageAllocator {
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

    /// # Safety
    /// The resulting tree's allocations must be aligned to
    /// [`PHYS_ALLOC_ALIGNMENT`](super::PHYS_ALLOC_ALIGNMENT).
    #[inline]
    pub unsafe fn push_tree(&mut self, base: PAddr, tree: *mut AllocTree) {
        self.trees.push(Entry { base, tree });
    }

    #[inline]
    pub fn sort_tree(&mut self) {
        self.trees.sort_unstable_by_key(|e| e.base);
    }

    pub fn alloc(&mut self, order: u32) -> Result<PAddr, TreeAllocError> {
        let mut last_error = TreeAllocError::InsufficientSpace { requested_order: order };
        for &Entry { base, tree } in &self.trees {
            let tree = unsafe { tree.as_mut_unchecked() };
            match tree.alloc(order) {
                Ok(index) => return Ok(base.byte_add(index as usize * PAGE_SIZE)),
                Err(e @ TreeAllocError::InsufficientSpace { .. }) => last_error = e,
            }
        }

        Err(last_error)
    }

    /// # Safety
    /// - `addr` must have been obtained through [`Self::alloc`].
    /// - `order` must be the same value passed through the same [`Self::alloc`] invocation.
    pub unsafe fn dealloc(&mut self, addr: PAddr, order: u32) {
        let tree_index = match self.trees.binary_search_by_key(&addr, |e| e.base) {
            Ok(i) => i,
            Err(i) => match i.checked_sub(1) {
                Some(i) => i,
                None => {
                    cold_path();
                    panic!("`PhysicalPageAllocator` has absolutely no trees");
                }
            },
        };

        unsafe {
            let &Entry { base, tree } = self.trees.get_unchecked(tree_index);
            let tree = tree.as_mut_unchecked();
            let index = u32::try_from((addr.addr() - base.addr()) / PAGE_SIZE).unwrap_unchecked();

            // `dealloc` *may* be called for pages that didn't originally come with this allocator
            // But in the case that they do, callers must ensure the safety invariants
            if index < tree.node_count() {
                tree.dealloc(index, order);
            }
        }
    }
}

unsafe impl Sync for PhysicalPageAllocator {}

#[derive(Debug, Clone, Copy)]
struct Entry {
    base: PAddr,
    tree: *mut AllocTree,
}
