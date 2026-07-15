#![no_std]

use elpytios_abi_macros::SyscallTable;

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
