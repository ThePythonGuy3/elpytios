#![feature(custom_inner_attributes)]
#![rustfmt::skip]

#![no_std]

use core::mem::MaybeUninit;

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

pub struct MemoryRegion {
    pub base: *mut u8,
    pub pages: usize
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootInfo {
    pub graphics_info:        GraphicsInfo,
    pub memory_regions_base: *const [MaybeUninit<MemoryRegion>; MAX_MEMORY_REGIONS],
    pub memory_regions_size:  usize
}
