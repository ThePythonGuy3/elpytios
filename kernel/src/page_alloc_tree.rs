/*use core::mem::offset_of;

#[repr(C)]
#[derive(Debug)]
pub struct BinaryBuddyTree {
    base:             *mut u8,
    pages:             usize,
    pub loader_memory: bool,
    children:         [BinaryBuddyTreeNode; 0]
}

const HAS_CHILDREN_FLAG: u8    = 0b0000_0001;
const IS_OCCUPIED_FLAG:  u8    = 0b0000_0010;
pub const PAGE_SIZE:     usize = 4096;

#[derive(Debug, Clone, Copy)]
struct BinaryBuddyTreeNode {
    flags: u8
}

const _: () = assert!(
    offset_of!(BinaryBuddyTree, children).is_multiple_of(align_of::<BinaryBuddyTreeNode>())
);

impl BinaryBuddyTree {
    fn get_child_start_and_pages(&self, node: usize) -> (usize, usize) {
        let level = (node + 1).ilog2() as usize;

        let first_in_level    = (1 << level) - 1;
        let position_in_level = node - first_in_level;

        let pages = self.pages >> level;
        let start = position_in_level * pages;

        debug_assert!(pages.is_power_of_two());

        (start, pages)
    }

    unsafe fn write_child(&mut self, offset: usize, node: &BinaryBuddyTreeNode) {
        unsafe {
            (self as *mut BinaryBuddyTree)
                .byte_add(offset_of!(Self, children))
                .cast::<BinaryBuddyTreeNode>()
                .add(offset)
                .write_volatile(*node);
        }
    }

    unsafe fn get_child(&self, offset: usize) -> *mut BinaryBuddyTreeNode {
        unsafe {
            (self as *const BinaryBuddyTree)
                .byte_add(offset_of!(Self, children))
                .cast::<BinaryBuddyTreeNode>()
                .cast_mut()
                .add(offset)
        }
    }

    /// Get the amount of necessary pages to store a tree that keeps track of `pages` pages.
    ///
    /// * `pages`: The amount of pages the tree will keep track of.
    pub fn required_pages_for_tree(pages: usize) -> usize {
        let required_size_bytes = size_of::<BinaryBuddyTree>() + (pages * 2 - 1) * size_of::<BinaryBuddyTreeNode>();

        return (required_size_bytes + (PAGE_SIZE - 1)) / PAGE_SIZE;
    }

    /// Create a new tree.
    ///
    /// * `tree_base`: Where to store the tree.
    /// * `memory_base`: Where the memory the tree keeps track of starts at.
    /// * `pages`: The amount of pages to keep track of.
    /// * `loader_memory`: Whether or not this tree is keeping memory in the UEFI LoaderCode or
    /// LoaderData regions.
    pub unsafe fn new(tree_base: *mut BinaryBuddyTree, memory_base: *mut u8, pages: usize, loader_memory: bool) -> Option<&'static mut Self> {
        unsafe {
            if pages == 0 || !pages.is_power_of_two() {
                return None;
            }

            if !(tree_base as usize).is_multiple_of(align_of::<BinaryBuddyTree>()) {
                return None;
            }

            tree_base.write_volatile(BinaryBuddyTree {
                base: memory_base,
                pages: pages,
                loader_memory: loader_memory,
                children: []
            });

            (&mut *tree_base).write_child(0, &BinaryBuddyTreeNode { flags: 0 });

            Some(&mut *tree_base)
        }
    }

    unsafe fn find_fitting_leaf_(&mut self, pages: usize, node: usize, min_pages: usize) -> Option<(usize, usize)> {
        unsafe {
            let child = self.get_child(node);

            if (*child).flags & HAS_CHILDREN_FLAG == 0 {
                let (_, child_pages) = self.get_child_start_and_pages(node);

                if (*child).flags & IS_OCCUPIED_FLAG == 0 &&
                    child_pages >= pages && child_pages < min_pages {
                    return Some((node, child_pages));
                } else {
                    return None;
                }
            } else {
                let result_left = self.find_fitting_leaf_(pages, node * 2 + 1, min_pages);

                let mut found_node   = None;
                let mut size_to_beat = min_pages;
                if let Some((node_left, min_pages_left)) = result_left {
                    size_to_beat = min_pages_left;
                    found_node   = Some(node_left);
                }

                let result_right = self.find_fitting_leaf_(pages, node * 2 + 2, size_to_beat);

                if let Some((node_right, min_pages_right)) = result_right {
                    size_to_beat = min_pages_right;
                    found_node   = Some(node_right);
                }

                if let Some(found_node) = found_node {
                    return Some((found_node, size_to_beat));
                } else {
                    return None;
                }
            }
        }
    }

    unsafe fn find_fitting_leaf(&mut self, pages: usize) -> Option<usize> {
        unsafe {
            if let Some((node, _)) = self.find_fitting_leaf_(pages, 0, usize::MAX) {
                return Some(node);
            }
        }

        return None;
    }

    unsafe fn split_node(&mut self, node: usize) -> (usize, usize) {
        unsafe {
            let base_child = self.get_child(node);
            (*base_child).flags |= HAS_CHILDREN_FLAG;

            self.write_child(node * 2 + 1, &BinaryBuddyTreeNode { flags: 0 });
            self.write_child(node * 2 + 2, &BinaryBuddyTreeNode { flags: 0 });

            (node * 2 + 1, node * 2 + 2)
        }
    }

    unsafe fn find_node_(&mut self, start: usize, node: usize) -> Option<usize> {
        unsafe {
            let child = self.get_child(node);

            let (child_start, child_pages) = self.get_child_start_and_pages(node);

            if start >= child_start && start < child_start + child_pages {
                if (*child).flags & HAS_CHILDREN_FLAG != 0 {
                    if start < child_start + child_pages / 2 {
                        return self.find_node_(start, node * 2 + 1);
                    } else {
                        return self.find_node_(start, node * 2 + 2);
                    }
                } else {
                    if child_start == start {
                        return Some(node);
                    } else {
                        return None;
                    }
                }
            }

            return None;
        }
    }

    unsafe fn find_node(&mut self, start: usize) -> Option<usize> {
        unsafe {
            return self.find_node_(start, 0);
        }
    }

    unsafe fn try_merge(&mut self, node: usize) {
        if node == 0 {
            return;
        }

        unsafe {
            let child = self.get_child(node);
            if (*child).flags & (HAS_CHILDREN_FLAG | IS_OCCUPIED_FLAG) != 0 {
                return;
            }

            let parent_node = (node - 1) / 2;

            let sibling;
            if node % 2 == 1 { // Left child
                sibling = self.get_child(node + 1);
            } else {
                sibling = self.get_child(node - 1);
            }

            if (*sibling).flags & (HAS_CHILDREN_FLAG | IS_OCCUPIED_FLAG) == 0 {
                (*self.get_child(parent_node)).flags &= !HAS_CHILDREN_FLAG;

                self.try_merge(parent_node);
            }
        }
    }

    /// Try to allocate `pages` consecutive pages.
    ///
    /// * `pages`: The amount of consecutive pages to allocate.
    pub unsafe fn alloc(&mut self, pages: usize) -> Option<*mut u8> {
        if pages == 0 {
            return None;
        }

        unsafe {
            if let Some(node) = self.find_fitting_leaf(pages) {
                let mut child_node = node;
                let mut child      = self.get_child(node);

                let (mut child_start, mut child_pages) = self.get_child_start_and_pages(node);

                while child_pages / 2 >= pages {
                    let (next_child, _) = self.split_node(child_node);

                    child_node = next_child;
                    child      = self.get_child(child_node);

                    (child_start, child_pages) = self.get_child_start_and_pages(child_node);
                }

                (*child).flags |= IS_OCCUPIED_FLAG;

                return Some((child_start * PAGE_SIZE + self.base as usize) as *mut u8);
            }
        }

        return None;
    }

    /// Try to free a region beginning at `page`.
    ///
    /// * `page`: The position of the region to free.
    pub unsafe fn free(&mut self, page: *mut u8) -> Result<(), ()> {
        debug_assert_eq!(page as usize % PAGE_SIZE, 0);

        let start = (page as usize - self.base as usize) / PAGE_SIZE;
        unsafe {
            if (page as usize) < (self.base as usize) || (page as usize) >= (self.base as usize) + self.pages * PAGE_SIZE {
                return Err(());
            }

            if let Some(node) = self.find_node(start) {
                let child = self.get_child(node);

                if (*child).flags & IS_OCCUPIED_FLAG != 0 {
                    (*child).flags &= !IS_OCCUPIED_FLAG;

                    self.try_merge(node);

                    return Ok(());
                }
            }
        }

        Err(())
    }
}*/
