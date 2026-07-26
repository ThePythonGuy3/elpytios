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
    #[args((file: FileHandle, buffer: *const u8, len: usize) => usize)]
    Write = 0x000,
    #[args((file: FileHandle, buffer: *mut u8, len: usize) => usize)]
    Read = 0x001,

    #[args((file: FileHandle, offset: usize, page_count: usize, flags: usize) => *mut u8)]
    MemMap = 0x010,
}

pub trait SyscallArg {
    fn into_usize(self) -> usize;

    unsafe fn from_usize(value: usize) -> Self;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct FileHandle(usize);
impl FileHandle {
    pub const STDOUT: Self = Self(0);
    pub const STDIN: Self = Self(1);

    pub const NONE: Self = Self(usize::MAX);
}

macro_rules! impl_syscall_arg {
    ($(impl $target:ty { get!($get:ident => $($getter:tt)*), set!($set:ident => $($setter:tt)*) })*) => {
        $(impl SyscallArg for $target {
            #[inline(always)]
            fn into_usize(self) -> usize {
                let $get = self;
                $($getter)*
            }

            #[inline(always)]
            unsafe fn from_usize($set: usize) -> Self {
                $($setter)*
            }
        })*
    };
}

impl_syscall_arg! {
    impl FileHandle { get!(this => this.0), set!(id => Self(id)) }
}

impl SyscallArg for usize {
    #[inline(always)]
    fn into_usize(self) -> usize {
        self
    }

    #[inline(always)]
    unsafe fn from_usize(value: usize) -> Self {
        value
    }
}

impl<T> SyscallArg for *const T {
    #[inline(always)]
    fn into_usize(self) -> usize {
        self as usize
    }

    #[inline(always)]
    unsafe fn from_usize(value: usize) -> Self {
        value as Self
    }
}

impl<T> SyscallArg for *mut T {
    #[inline(always)]
    fn into_usize(self) -> usize {
        self as usize
    }

    #[inline(always)]
    unsafe fn from_usize(value: usize) -> Self {
        value as Self
    }
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
