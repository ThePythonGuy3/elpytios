#![feature(custom_inner_attributes)]
#![rustfmt::skip]

#![no_std]

pub mod buddy_tree;
pub mod paddr;
pub mod vaddr {
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

pub const PAGE_SIZE: usize = 4096;

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

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootInfo {
    pub graphics_info: GraphicsInfo
}
