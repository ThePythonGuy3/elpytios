#![feature(custom_inner_attributes)]
#![rustfmt::skip]

#![no_std]
#![no_main]

use core::{fmt::Write, panic::PanicInfo};

use elpytios_bootinfo::{BootInfo, GraphicsInfo};
use elpytios_kernel::rendering::DisplayWriter;

#[panic_handler]
fn hanic_pandler(_info: &PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
unsafe extern "sysv64" fn _start(boot_info: *mut BootInfo) -> ! {
    let mut display_writer;

    unsafe {
        display_writer = DisplayWriter {
            graphics_info: &((*boot_info).graphics_info),
            line: 0,
            col: 0,
        };
    }

    writeln!(&mut display_writer, "Hello World from the Kernel!!!!").unwrap();

    loop {}
}
