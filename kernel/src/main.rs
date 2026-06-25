#![feature(custom_inner_attributes)]
#![rustfmt::skip]

#![no_std]
#![no_main]

use core::{arch::naked_asm, fmt::Write, panic::PanicInfo};

use elpytios_bootinfo::{BootInfo, PAGE_SIZE};
use elpytios_kernel::rendering::DisplayWriter;

#[panic_handler]
fn hanic_pandler(_info: &PanicInfo) -> ! {
    loop {}
}

#[cfg(debug_assertions)]
#[unsafe(no_mangle)]
#[used]
static mut DEBUG_HALT: u8 = 1;

#[cfg(debug_assertions)]
#[inline(never)]
fn pause() {
    loop {
        if unsafe { DEBUG_HALT } == 0 {
            break
        }

        core::hint::spin_loop();
    }
}

#[unsafe(naked)]
#[unsafe(export_name = "_start")]
unsafe extern "sysv64" fn jump_from_bootloader(boot_info: &'static BootInfo) -> ! {
    naked_asm!(
        "jmp {main}",
        main = sym main
    )
}

unsafe extern "sysv64" fn main(boot_info: &'static BootInfo) -> ! {
    #[cfg(debug_assertions)]
    pause();

    let mut display_writer = DisplayWriter {
        graphics_info: &boot_info.graphics_info,
        line: 0,
        col: 0,
    };

    writeln!(&mut display_writer, "Hello World from the Kernel, calling at address {:p}!!!!", main as *const ()).unwrap();

    let [region, regions @ ..] = boot_info.memory_regions() else {
        panic!("No usable RAM")
    };
    let mut region = *region;
    let mut regions = regions;

    while let [next, next_regions @ ..] = regions {
        if region.base.byte_add(region.pages * PAGE_SIZE) == next.base {
            region.pages += next.pages;
        } else {
            writeln!(&mut display_writer, "Usable memory block [{}..{}], {} pages", region.base, region.base.byte_add(region.pages * PAGE_SIZE), region.pages).unwrap();
            region = *next;
        }
        regions = next_regions;
    }
    writeln!(&mut display_writer, "Usable memory block [{}..{}], {} pages", region.base, region.base.byte_add(region.pages * PAGE_SIZE), region.pages).unwrap();

    loop {}
}