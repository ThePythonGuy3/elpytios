use core::ptr::NonNull;

use elpytios_alloc::PageAllocator;

use crate::{
    statics::{get_phys_alloc, phys_to_virt, virt_to_phys},
    vaddr::VAddr,
};

mod phys;
mod tree;
pub use phys::*;
pub use tree::*;

#[derive(Debug, Clone, Copy)]
pub struct KernelPageAllocator;
unsafe impl PageAllocator for KernelPageAllocator {
    #[inline]
    fn alloc(&self, order: u32) -> Option<NonNull<u8>> {
        get_phys_alloc()
            .lock()
            .alloc(order)
            .ok()
            .map(|addr| unsafe { NonNull::new_unchecked(phys_to_virt(addr).ptr_mut::<u8>()) })
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: NonNull<u8>, order: u32) {
        unsafe { get_phys_alloc().lock().dealloc(virt_to_phys(VAddr::new(ptr.as_ptr() as usize)), order) }
    }
}
