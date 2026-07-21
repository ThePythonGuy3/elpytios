use crate::alloc::Layout;

pub unsafe fn alloc(layout: Layout) -> *mut u8 {
    unimplemented!("{layout:?}")
}

pub unsafe fn dealloc(ptr: *mut u8, layout: Layout) {
    unimplemented!("{ptr:p} -> {layout:?}")
}

pub unsafe fn alloc_zeroed(layout: Layout) -> *mut u8 {
    unimplemented!("{layout:?}")
}

pub unsafe fn realloc(ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
    unimplemented!("{ptr:p} -> {layout:?} -> {new_size}")
}
