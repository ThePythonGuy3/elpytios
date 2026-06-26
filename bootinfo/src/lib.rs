#![feature(custom_inner_attributes)]
#![rustfmt::skip]

#![no_std]

pub mod paddr;
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

    #[derive(Debug, Display, Clone, Copy)]
    #[repr(C)]
    pub enum VirtualMapError {
        #[display("Couldn't allocate a page table")]
        PageTable,
        #[display("Couldn't map {v_addr} to {p_addr}: the virtual address is reserved")]
        Reserved { p_addr: PAddr, v_addr: VAddr },
        #[display("Couldn't map {v_addr} to {p_addr}: the virtual address is already mapped to {p_addr_existing}")]
        AlreadyMapped { p_addr: PAddr, v_addr: VAddr, p_addr_existing: PAddr }
    }

    cfg_select! {
        target_arch = "x86_64" => {
            mod x86_64;
            pub use x86_64::*;
        }
        _ => {
            compile_error!("Unsupported architecture");
        }
    }
}

use bitflags::bitflags;
use derive_more::Display;

use core::mem::MaybeUninit;

use paddr::PAddr;
use vaddr::{VAddr, VirtualMap};

pub const PAGE_SIZE: usize = 4096;
pub const MAX_MEMORY_REGIONS: usize = 128;

#[allow(non_camel_case_types)]
#[derive(Clone, Copy)]
#[repr(usize)]
pub enum PixelFormat {
    RGB_8_BIT,
    BGR_8_BIT,
    BIT_MASK,
    BLT_ONLY
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GraphicsInfo {
    pub w:                 usize,
    pub h:                 usize,
    pub stride:            usize,
    pub pixel_format:      PixelFormat,
    pub frame_buffer:     *mut u8,
    pub frame_buffer_size: usize
}

#[derive(Clone, Copy)]
pub struct MemoryRegion {
    pub base:  PAddr,
    pub pages: usize
}

#[repr(C, align(4096))]
pub struct BootInfo {
    pub graphics_info:        GraphicsInfo,
    pub virtual_map:          VirtualMap,
    /// Leftover identity-mapping from the bootloader, to be unmapped by the kernel
    pub switcher_map:         VAddr,

    pub memory_regions_base:  [MaybeUninit<MemoryRegion>; MAX_MEMORY_REGIONS],
    pub memory_regions_size:  usize
}