use core::fmt;

use elpytios_bootinfo::PAGE_SIZE;
use elpytios_elf::ElfError;

use crate::{allocator::TreeAllocError, vaddr::VirtualMapError};

cfg_select! {
    target_arch = "x86_64" => {
        mod x86_64;
        pub use x86_64::*;
    }
    _ => {
        compile_error!("Unsupported architecture");
    }
}

mod queue;
pub use queue::*;

pub enum TaskCreateError {
    Elf(ElfError),
    Alloc(TreeAllocError),
    VMap(VirtualMapError),
    NonRelocatable,
    TooManySegments,
    InvalidAlignment(u64),
}

impl fmt::Debug for TaskCreateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for TaskCreateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Elf(e) => write!(f, "Couldn't parse ELF for task creation: {e}"),
            Self::Alloc(e) => write!(f, "Couldn't allocate memory for program segment: {e}"),
            Self::VMap(e) => write!(f, "Couldn't virtual-map program segment: {e}"),
            Self::NonRelocatable => write!(f, "ELF is non-relocatable; recompile the program with -fPIE"),
            Self::TooManySegments => write!(f, "ELF has too many program segments, maximum is 11"),
            Self::InvalidAlignment(align) => write!(f, "ELF program segment alignment isn't {PAGE_SIZE} ({align})"),
        }
    }
}

impl From<ElfError> for TaskCreateError {
    #[inline]
    fn from(value: ElfError) -> Self {
        Self::Elf(value)
    }
}

impl From<TreeAllocError> for TaskCreateError {
    #[inline]
    fn from(value: TreeAllocError) -> Self {
        Self::Alloc(value)
    }
}

impl From<VirtualMapError> for TaskCreateError {
    #[inline]
    fn from(value: VirtualMapError) -> Self {
        Self::VMap(value)
    }
}
