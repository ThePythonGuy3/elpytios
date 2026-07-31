use elpytios_abi::{FileHandle, Syscall};
use elpytios_alloc::{HeapAllocator, PageAllocator};

use crate::{
    alloc::{GlobalAlloc, Layout},
    ptr::NonNull,
};

#[global_allocator]
static IMPL: HeapAllocator<StdPageAllocator> = HeapAllocator::new(StdPageAllocator);

struct StdPageAllocator;
unsafe impl PageAllocator for StdPageAllocator {
    #[inline]
    fn alloc(&self, order: u32) -> Option<NonNull<u8>> {
        unsafe { NonNull::new(Syscall::mem_map(FileHandle::NONE, 0, 1 << order, 0)) }
    }

    // TODO unmap syscall
    #[inline]
    unsafe fn dealloc(&self, _ptr: NonNull<u8>, _order: u32) {}
}

#[inline]
pub unsafe fn alloc(layout: Layout) -> *mut u8 {
    unsafe { IMPL.alloc(layout) }
}

#[inline]
pub unsafe fn dealloc(ptr: *mut u8, layout: Layout) {
    unsafe { IMPL.dealloc(ptr, layout) }
}

#[inline]
pub unsafe fn alloc_zeroed(layout: Layout) -> *mut u8 {
    unsafe { IMPL.alloc_zeroed(layout) }
}

#[inline]
pub unsafe fn realloc(ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
    unsafe { IMPL.realloc(ptr, layout, new_size) }
}
