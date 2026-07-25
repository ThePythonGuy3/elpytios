#![feature(ptr_alignment_type)]
#![no_std]

use core::mem::Alignment;

use elpytios_abi_macros::SyscallTable;

pub const PAGE_SIZE: usize = 4096;
/// Physical allocators are aligned to 32 pages (128 KiB).
/// This means pointers of allocations up to 32 pages are guaranteed to be aligned.
pub const ALLOC_ALIGNMENT: Alignment = unsafe { Alignment::new_unchecked(1 << (32 * PAGE_SIZE).ilog2()) };

#[derive(Debug, Copy, Clone, PartialEq, Eq, SyscallTable)]
#[max_entries(4096)]
pub enum Syscall {
    #[args(file, buffer, len)]
    Write = 0,
    #[args(file, buffer, len)]
    Read = 1,
}

#[expect(unused, reason = "Not all parameters are used yet")]
mod kernel {
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

#[expect(unused, reason = "Not all parameters are used yet")]
mod userspace {
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
