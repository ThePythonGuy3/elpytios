use crate::arch::naked_asm;

/// SAFETY: must be called only once during runtime initialization.
pub unsafe fn init(_argc: isize, _argv: *const *const u8, _sigpipe: u8) {}

/// SAFETY: must be called only once during runtime cleanup.
pub unsafe fn cleanup() {}

pub fn abort_internal() -> ! {
    crate::intrinsics::abort()
}

unsafe extern "C" {
    fn main(argc: i32, argv: *const *const u8) -> i32;
}

cfg_select! {
    target_arch = "x86_64" => {
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "sysv64" fn _start() -> ! {
            naked_asm!(
                "movq $0, %rdi",
                "movq %rsp, %rsi",
                "jmp {main}",

                main = sym main,

                options(att_syntax),
            )
        }
    }
    _ => {
        compile_error!("Unsupported architecture");
    }
}
