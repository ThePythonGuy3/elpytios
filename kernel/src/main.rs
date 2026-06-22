#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[panic_handler]
fn hanic_pandler(_info: &PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
unsafe extern "sysv64" fn _start() -> ! {
    loop {}
}
