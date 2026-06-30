use core::{
    fmt,
    sync::atomic::{AtomicBool, Ordering::Relaxed},
};

use arrayvec::ArrayVec;
use elpytios_bootinfo::{PAGE_SIZE, paddr::PAddr};

use crate::alloc::{AllocTree, TreeAllocError, TreeAllocId};

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

    pub fn alloc(&mut self, page_count: usize) -> Result<AllocId, TreeAllocError> {
        let mut last_error = TreeAllocError::InsufficientSpace { requested: page_count };
        for (tree_index, &Entry { base, tree }) in self.trees.iter().enumerate() {
            let tree = unsafe { tree.as_mut_unchecked() };
            match tree.alloc(page_count) {
                Ok(tree_id) => {
                    return Ok(AllocId {
                        tree_id,
                        tree_index,
                        addr: base.byte_add(tree_id.index() as usize * PAGE_SIZE),
                    })
                }
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

#[derive(Clone, Copy)]
pub struct AllocId {
    tree_id: TreeAllocId,
    tree_index: usize,
    addr: PAddr,
}

impl fmt::Debug for AllocId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AllocId")
            .field("addr", &self.addr)
            .field("page_count", &self.page_count())
            .field("byte_len", &self.byte_len())
            .finish_non_exhaustive()
    }
}

impl AllocId {
    #[inline]
    pub const fn addr(&self) -> PAddr {
        self.addr
    }

    #[inline]
    pub const fn page_count(&self) -> usize {
        1 << self.tree_id.order()
    }

    #[inline]
    pub const fn byte_len(&self) -> usize {
        self.page_count() * PAGE_SIZE
    }
}
