#![feature(arbitrary_self_types_pointers, const_trait_impl, const_try, ptr_metadata, slice_ptr_get, sync_unsafe_cell)]
#![no_std]

pub mod allocator;
pub mod framebuffer;
pub mod rendering;
pub mod serial {
    cfg_select! {
        target_arch = "x86_64" => {
            mod uart;
            pub use uart::*;
        }
        _ => {
            compile_error!("Unsupported architecture");
        }
    }
}
pub mod spin_sync;
pub mod vaddr {
    use super::*;

    #[derive(Debug, Clone, Copy)]
    #[repr(transparent)]
    pub struct VFlags(usize);
    bitflags! {
        impl VFlags: usize {
            const WRITABLE        = 1 << 0;
            const USER_MODE       = 1 << 1;
            const WRITE_THROUGH   = 1 << 2;
            const CACHE_DISABLED  = 1 << 3;
            const ACCESSED        = 1 << 4;

            /// Don't flush translation lookaside buffers when switching virtual map tables
            const GLOBAL          = 1 << 5;
        }
    }

    #[derive(Display, Clone, Copy)]
    #[repr(C)]
    pub enum VirtualMapError {
        #[display("Couldn't allocate a page table")]
        PageTable,
        #[display("Couldn't map {v_addr:p} to {p_addr:p}: the virtual address is reserved")]
        Reserved { p_addr: PAddr, v_addr: VAddr },
        #[display("Couldn't map {v_addr:p} to {p_addr:p}: the virtual address is already mapped to {p_addr_existing:p}")]
        AlreadyMapped { p_addr: PAddr, v_addr: VAddr, p_addr_existing: PAddr },
    }

    impl fmt::Debug for VirtualMapError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            fmt::Display::fmt(self, f)
        }
    }

    pub(crate) use imp::*;
    pub use imp::{VAddr, VirtualMap, VirtualMapBuilder};

    cfg_select! {
        target_arch = "x86_64" => {
            mod x86_64;
            use x86_64 as imp;
        }
        _ => {
            compile_error!("Unsupported architecture");
        }
    }
}

use core::{fmt, mem::MaybeUninit};

use allocator::PhysicalPageAllocator;
use bitflags::bitflags;
use derive_more::Display;
use elpytios_bootinfo::paddr::PAddr;
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
