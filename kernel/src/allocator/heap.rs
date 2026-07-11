use core::alloc::{GlobalAlloc, Layout};

/// Simple XOR linked-list allocator.
///
/// # Safety
/// - Can only be used once physical page allocator and virtual map is set up.
//TODO replace with a real malloc implementation once eldo is done with it
pub struct HeapAllocator {}
impl HeapAllocator {
    pub const fn new() -> Self {
        Self {}
    }
}

unsafe impl GlobalAlloc for HeapAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        todo!()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        todo!()
    }
}
