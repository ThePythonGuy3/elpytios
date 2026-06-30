#![feature(
    arbitrary_self_types_pointers,
    const_trait_impl,
    const_try,
    custom_inner_attributes,
    ptr_metadata,
    slice_ptr_get
)]
#![no_std]

pub mod alloc;
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

use bitflags::bitflags;
use derive_more::Display;
use elpytios_bootinfo::paddr::PAddr;

use crate::vaddr::VirtualMap;

/// # Safety
/// Every single one of these statics must be set by their corresponding `set_*` functions below in
/// the setup-phase of the kernel.
///
/// See `main.rs`.
pub mod statics {
    use super::*;

    static mut VIRTUAL_MAP: MaybeUninit<VirtualMap> = MaybeUninit::uninit();

    #[inline]
    pub unsafe fn set_virtual_map(virtual_map: VirtualMap) {
        unsafe {
            statics::VIRTUAL_MAP = MaybeUninit::new(virtual_map);
        }
    }

    #[inline]
    pub fn get_virtual_map() -> &'static VirtualMap {
        unsafe { (&raw const statics::VIRTUAL_MAP as *const VirtualMap).as_ref_unchecked() }
    }
}
