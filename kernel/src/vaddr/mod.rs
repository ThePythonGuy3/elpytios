pub(crate) use imp::*;
pub use imp::{VAddr, VirtualMap, VirtualMapBuilder};

cfg_select! {
    target_arch = "x86_64" => {
        mod x86_64;
        use x86_64 as imp;
    }
    _ => {
        compile_error!("Unsupported architecture");
    }
}

use core::fmt;

use bitflags::bitflags;
use elpytios_bootinfo::paddr::PAddr;

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct VFlags(usize);
bitflags! {
    impl VFlags: usize {
        const WRITABLE        = 1 << 0;
        const USER_MODE       = 1 << 1;
        const WRITE_THROUGH   = 1 << 2;
        const CACHE_DISABLED  = 1 << 3;
        const ACCESSED        = 1 << 4;

        /// Don't flush translation lookaside buffers when switching virtual map tables
        const GLOBAL          = 1 << 5;
        const EXECUTE_DISABLE = 1 << 6;
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
pub enum VirtualMapError {
    PageTable,
    AlreadyMapped { p_addr: PAddr, v_addr: VAddr, p_addr_existing: PAddr },
}

impl fmt::Debug for VirtualMapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for VirtualMapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
<<<<<<< HEAD
            Self::PageTable => writeln!(f, "Couldn't allocate a page table"),
=======
            Self::PageTable => write!(f, "Couldn't allocate a page table"),
            Self::Reserved { p_addr, v_addr } => write!(f, "Couldn't map {v_addr:p} to {p_addr:p}: the virtual address is reserved"),
>>>>>>> 4dccca6 (Parse ACPI headers)
            Self::AlreadyMapped {
                p_addr,
                v_addr,
                p_addr_existing,
            } => write!(
                f,
                "Couldn't map {v_addr:p} to {p_addr:p}: the virtual address is already mapped to {p_addr_existing:p}"
            ),
        }
    }
}
