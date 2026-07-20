use crate::alloc::Layout;

pub unsafe fn alloc(layout: Layout) -> *mut u8 {
    unimplemented!()
}

pub unsafe fn dealloc(ptr: *mut u8, layout: Layout) {
    unimplemented!()
}

pub unsafe fn alloc_zeroed(layout: Layout) -> *mut u8 {
    unimplemented!()
}

pub unsafe fn realloc(ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
    unimplemented!()
}
