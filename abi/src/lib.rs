#![no_std]

use elpytios_abi_macros::SyscallTable;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(usize)]
#[derive(SyscallTable)]
pub enum Syscall {
    #[args(file, buffer, len)]
    Write = 0,
    #[args(file, buffer, len)]
    Read = 1,
}

#[cfg(not(target_os = "none"))]
#[expect(
    unused,
    reason = "Not all args are used yet, they will be in the future. Remove this `expect()` when that happens."
)]
pub(crate) mod userspace {
    cfg_select! {
        target_arch = "x86_64" => {
            mod x86_64;
            pub use x86_64::*;
        }
        _ => {
            mod unsupported;
            pub use unsupported::*;
        }
    }
}
