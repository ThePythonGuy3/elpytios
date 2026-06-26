#![feature(custom_inner_attributes)]
#![rustfmt::skip]

#![no_std]

pub mod page_alloc;
pub mod page_alloc_tree;
pub mod rendering;

use core::{mem::MaybeUninit, slice};

use elpytios_bootinfo::{BootInfo, GraphicsInfo, MemoryRegion};

#[unsafe(link_section = ".bootinfo")]
#[used]
static mut BOOT_INFO: MaybeUninit<BootInfo> = MaybeUninit::uninit();

#[inline]
pub fn graphics_info() -> &'static GraphicsInfo {
    unsafe { &(*(&raw const BOOT_INFO as *const BootInfo)).graphics_info }
}

#[inline]
pub fn memory_regions() -> &'static [MemoryRegion] {
    unsafe {
        let boot_info_ptr = &raw const BOOT_INFO as *const BootInfo;
        slice::from_raw_parts(
            &raw const (*boot_info_ptr).memory_regions_base as *const MemoryRegion,
            (*boot_info_ptr).memory_regions_size,
        )
    }
}
