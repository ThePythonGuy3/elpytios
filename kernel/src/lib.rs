#![feature(arbitrary_self_types_pointers, const_trait_impl, const_try, custom_inner_attributes, ptr_metadata, slice_ptr_get)]
#![rustfmt::skip]

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

use core::mem::MaybeUninit;

use elpytios_bootinfo::BootInfo;

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        _ = write!($crate::serial::Serial($crate::serial::Com::Com3), $($arg)*);
    }};
}

#[macro_export]
macro_rules! println {
    () => {
        $crate::print!("\n")
    };
    ($($arg:tt)*) => {{
        $crate::print!($($arg)*);
        $crate::print!("\n");
    }};
}

#[unsafe(link_section = ".bootinfo")]
#[used]
static BOOT_INFO: MaybeUninit<BootInfo> = MaybeUninit::uninit();

pub fn boot_info() -> &'static BootInfo {
    unsafe { (&raw const BOOT_INFO as *const BootInfo).as_ref_unchecked() }
}