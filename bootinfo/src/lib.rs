#![feature(custom_inner_attributes)]
#![rustfmt::skip]

#![no_std]

pub mod paddr;
pub mod vaddr {
    cfg_select! {
        target_arch = "x86_64" => {
            // TODO Only re-export public-facing APIs
            mod x86_64;
            pub use x86_64::*;
        }
        _ => {
            compile_error!("Unsupported architecture");
        }
    }
}

use core::{mem::MaybeUninit, slice};

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

#[derive(Clone, Copy)]
#[repr(C, align(4096))]
pub struct BootInfo {
    pub graphics_info:        GraphicsInfo,
    pub virtual_map:          VirtualMap,
    /// Leftover identity-mapping from the bootloader
    pub switcher_map:         VAddr,

    pub memory_regions_base:  [MaybeUninit<MemoryRegion>; MAX_MEMORY_REGIONS],
    pub memory_regions_size:  usize
}

impl BootInfo {
    #[inline]
    pub fn memory_regions(&self) -> &[MemoryRegion] {
        unsafe { slice::from_raw_parts(self.memory_regions_base.as_ptr() as _, self.memory_regions_size) }
    }
}
