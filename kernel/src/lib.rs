#![feature(custom_inner_attributes)]
#![rustfmt::skip]

#![no_std]

pub mod page_alloc;
pub mod page_alloc_tree;
pub mod rendering;

use core::mem::MaybeUninit;

use elpytios_bootinfo::BootInfo;

#[unsafe(link_section = ".bootinfo")]
#[used]
static mut BOOT_INFO: MaybeUninit<BootInfo> = MaybeUninit::uninit();

pub fn boot_info() -> &'static BootInfo {
    unsafe { (&raw const BOOT_INFO as *const BootInfo).as_ref_unchecked() }
}