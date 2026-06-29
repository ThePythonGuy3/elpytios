#![feature(custom_inner_attributes)]
#![rustfmt::skip]

#![no_std]

pub mod paddr;

use arrayvec::ArrayVec;
use paddr::PAddr;

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
pub struct MemoryRegion {
    pub base:    PAddr,
    pub pages:   usize,
}

// Note: Must uphold `BootInfo: Sync`
#[derive(Debug)]
#[repr(C, align(4096))]
pub struct BootInfo {
    //pub kernel_base:           PAddr,
    //pub kernel_pages:          usize,
    pub page_table_init:     PAddr,
    pub page_table_init_len: usize,
    pub memory_regions:      ArrayVec<MemoryRegion, MAX_MEMORY_REGIONS>,
}