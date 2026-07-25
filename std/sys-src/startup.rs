use crate::ffi::c_int;

/// SAFETY: must be called only once during runtime initialization.
pub unsafe fn init(_argc: isize, _argv: *const *const u8, _sigpipe: u8) {}

/// SAFETY: must be called only once during runtime cleanup.
pub unsafe fn cleanup() {}

pub fn abort_internal() -> ! {
    loop {}
}

// Compiler-generated shim
unsafe extern "C" {
    safe fn main(argc: c_int, argv: *const *const u8) -> c_int;
}

cfg_select! {
    target_arch = "x86_64" => {
        #[unsafe(no_mangle)]
        pub unsafe extern "sysv64" fn _start(argc: c_int, argv: *const *const u8) -> ! {
            let _exit_code = main(argc, argv);
            loop {}
        }
    }
    _ => {
        compile_error!("Unsupported architecture");
    }
}
