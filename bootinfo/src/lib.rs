#![feature(custom_inner_attributes)]
#![rustfmt::skip]
#![no_std]

pub mod paddr;

use arrayvec::ArrayVec;
use bitflags::bitflags;
use paddr::PAddr;

pub const PAGE_SIZE: usize = 4096;
pub const MAX_MEMORY_REGIONS: usize = 128;
pub const MAX_IDENTITY_MAP: usize = 32;

#[allow(non_camel_case_types)]
#[derive(Clone, Copy)]
#[repr(usize)]
pub enum PixelFormat {
    RGB_8_BIT,
    BGR_8_BIT,
    BIT_MASK,
    BLT_ONLY
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct GraphicsInfo {
    pub w:                 usize,
    pub h:                 usize,
    pub stride:            usize,
    pub pixel_format:      PixelFormat,
    pub frame_buffer:     *mut u8,
    pub frame_buffer_size: usize
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct MemoryRegion {
    pub base:  PAddr,
    pub pages: usize,
}

impl MemoryRegion {
    #[inline]
    pub const fn at(base: PAddr, pages: usize) -> Self {
        Self { base, pages }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct IdentityMap {
    pub region: MemoryRegion,
    pub flags: IdentityMapFlags,
}

bitflags! {
    #[derive(Debug, Clone, Copy)]
    #[repr(transparent)]
    pub struct IdentityMapFlags: u8 {
        const EXECUTABLE = 1 << 0;
        const WRITABLE   = 1 << 1;
        const READABLE   = 1 << 2;
    }
}

impl IdentityMap {
    #[inline]
    pub const fn new(base: PAddr, pages: usize, flags: IdentityMapFlags) -> Self {
        Self {
            region: MemoryRegion { base, pages },
            flags,
        }
    }
}

// Note: Must uphold `BootInfo: Sync`
#[derive(Debug)]
#[repr(C, align(4096))]
pub struct BootInfo {
    /// Used to calculate slide for virtual mapping
    pub kernel_base:         PAddr,
    pub page_table_init:     PAddr,
    pub page_table_init_len: usize,
    pub memory_regions:      ArrayVec<MemoryRegion, MAX_MEMORY_REGIONS>,
    pub identity_maps:       ArrayVec<IdentityMap, MAX_IDENTITY_MAP>,
}