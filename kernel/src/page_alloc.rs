/*use core::{fmt::{Display, Formatter}, mem::offset_of, ptr::null_mut};

use uefi::{boot::MemoryType, mem::memory_map::{MemoryMap, MemoryMapOwned}};

use crate::page_alloc_tree::{BinaryBuddyTree, PAGE_SIZE};

const MINIMUM_PAGES_TO_MANAGE:          usize = 4;
const PAGES_RESERVED_FOR_TREE_POINTERS: usize = 2; // TODO Fix, This is STUPID

#[repr(C)]
pub struct PhysicalPageAllocator {
    size:        usize,
    trees: [*mut BinaryBuddyTree; 0]
}

impl PhysicalPageAllocator {
    unsafe fn push_tree(&mut self, tree: *mut BinaryBuddyTree) {
        unsafe {
            (self as *mut Self)
                .byte_add(offset_of!(Self, trees))
                .cast::<*mut BinaryBuddyTree>()
                .add(self.size)
                .write_volatile(tree);
        }

        self.size += 1;
    }

    unsafe fn get_tree(&self, n: usize) -> *mut BinaryBuddyTree {
        unsafe {
            (self as *const Self)
                .byte_add(offset_of!(Self, trees))
                .cast::<*mut BinaryBuddyTree>()
                .add(n)
                .cast_mut()
                .read_volatile()
        }
    }

    unsafe fn create_trees_(&mut self, start: usize, pages: usize, loader_memory: bool) -> Result<(), ()> {
        if pages < MINIMUM_PAGES_TO_MANAGE {
            return Err(());
        }

        let mut region_size = 1 << (pages.ilog2() as usize);
        let free_pages = pages - region_size;

        let mut required_pages_for_tree = BinaryBuddyTree::required_pages_for_tree(region_size);

        while required_pages_for_tree > free_pages {
            region_size >>= 1;

            if region_size < MINIMUM_PAGES_TO_MANAGE {
                return Err(());
            }

            required_pages_for_tree = BinaryBuddyTree::required_pages_for_tree(region_size);
        }

        let memory_region_start = start + required_pages_for_tree * PAGE_SIZE;

        unsafe {
            if let Some(tree) = BinaryBuddyTree::new(
                start as *mut BinaryBuddyTree,
                memory_region_start as *mut u8,
                region_size,
                loader_memory
            ) {
                self.push_tree(tree);

                if let Ok(_) = self.create_trees_(
                    memory_region_start + region_size * PAGE_SIZE,
                    pages - region_size - required_pages_for_tree,
                    loader_memory
                ) {
                    return Ok(());
                } else {
                    return Err(());
                }
            } else {
                return Err(());
            }
        }
    }

    pub fn new(memory_map: &MemoryMapOwned) -> Result<&mut Self, ()> {
        let mut allocator: *mut Self = null_mut();

        for i in memory_map.entries() {
            let valid;
            let loader_memory;
            match i.ty {
                MemoryType::CONVENTIONAL |
                MemoryType::PERSISTENT_MEMORY => {
                    valid         = true;
                    loader_memory = false;
                },
                MemoryType::BOOT_SERVICES_CODE |
                MemoryType::BOOT_SERVICES_DATA |
                MemoryType::LOADER_CODE        |
                MemoryType::LOADER_DATA => {
                    valid         = false;
                    loader_memory = true;
                }
                _ => {
                    valid         = false;
                    loader_memory = false;
                }
            }

            if valid {
                let mut start = i.phys_start as usize;
                let mut size  = i.page_count as usize;

                if allocator.is_null() && start.is_multiple_of(align_of::<Self>()) &&
                    size == PAGES_RESERVED_FOR_TREE_POINTERS {
                    allocator = start as *mut Self;

                    unsafe {
                        allocator.write_volatile(PhysicalPageAllocator { size: 0, trees: [] });
                    }

                    start = start + PAGE_SIZE * PAGES_RESERVED_FOR_TREE_POINTERS;
                    size -= PAGES_RESERVED_FOR_TREE_POINTERS;
                }

                if !allocator.is_null() && size >= MINIMUM_PAGES_TO_MANAGE {
                    unsafe {
                        let _ = (&mut*allocator).create_trees_(start, size, loader_memory);
                    }
                }
            }
        }

        unsafe {
            if allocator.is_null() {
                Err(())
            } else {
                Ok(&mut*allocator)
            }
        }
    }

    pub unsafe fn alloc(&mut self, pages: usize) -> Option<*mut u8> {
        for i in 0..self.size {
            unsafe {
                if let Some(address) = (*self.get_tree(i)).alloc(pages) {
                    return Some(address);
                }
            }
        }

        return None;
    }

    pub unsafe fn free(&mut self, _region: *mut u8) -> Result<(), ()> {
        Ok(())
    }
}

impl Display for PhysicalPageAllocator {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        for i in 0..self.size {
            unsafe {
                writeln!(f, "{:?}", self.get_tree(i))?;
            }
        }

        Ok(())
    }
}*/
