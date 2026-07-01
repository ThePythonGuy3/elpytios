#![forbid(unfulfilled_lint_expectations)]
#![feature(arbitrary_self_types_pointers, const_trait_impl, const_try, ptr_metadata, slice_ptr_get, sync_unsafe_cell)]
#![no_std]

pub mod allocator;
pub mod framebuffer;
pub mod rendering;
pub mod serial;
pub mod spin_sync;
pub mod vaddr;

use core::mem::MaybeUninit;

use allocator::PhysicalPageAllocator;
use framebuffer::FrameBuffer;
use spin_sync::SpinMutex;
use vaddr::VirtualMap;

/// # Safety
/// Every single one of these statics must be set by their corresponding `set_*` functions below in
/// the setup-phase of the kernel.
///
/// See `main.rs`.
pub mod statics {
    use super::*;

    static mut VIRTUAL_MAP: MaybeUninit<VirtualMap> = MaybeUninit::uninit();
    static mut PHYS_ALLOC: MaybeUninit<SpinMutex<PhysicalPageAllocator>> = MaybeUninit::uninit();
    static mut FRAME_BUFFER: MaybeUninit<FrameBuffer> = MaybeUninit::uninit();

    #[inline]
    pub unsafe fn set_virtual_map(virtual_map: VirtualMap) {
        unsafe {
            VIRTUAL_MAP = MaybeUninit::new(virtual_map);
        }
    }

    #[inline]
    pub unsafe fn set_phys_alloc(phys_alloc: PhysicalPageAllocator) {
        unsafe {
            PHYS_ALLOC = MaybeUninit::new(SpinMutex::new(phys_alloc));
        }
    }

    #[inline]
    pub unsafe fn set_frame_buffer(frame_buffer: FrameBuffer) {
        unsafe {
            FRAME_BUFFER = MaybeUninit::new(frame_buffer);
        }
    }

    #[inline]
    pub fn get_virtual_map() -> &'static VirtualMap {
        unsafe { (&raw const VIRTUAL_MAP as *const VirtualMap).as_ref_unchecked() }
    }

    #[inline]
    pub fn get_phys_alloc() -> &'static SpinMutex<PhysicalPageAllocator> {
        unsafe { (&raw const PHYS_ALLOC as *const SpinMutex<PhysicalPageAllocator>).as_ref_unchecked() }
    }

    #[inline]
    pub fn get_frame_buffer() -> &'static FrameBuffer {
        unsafe { (&raw const FRAME_BUFFER as *const FrameBuffer).as_ref_unchecked() }
    }
}
