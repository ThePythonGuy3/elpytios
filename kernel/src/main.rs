#![feature(custom_inner_attributes)]
#![rustfmt::skip]

#![no_std]
#![no_main]

use core::{arch::{asm, naked_asm}, fmt::Write, panic::PanicInfo};

use elpytios_bootinfo::{PAGE_SIZE, paddr::PAddr};
use elpytios_kernel::{boot_info, rendering::DisplayWriter};

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
unsafe extern "sysv64" fn jump_from_bootloader(root_page_table: PAddr, kernel_offset: usize) -> ! {
    naked_asm!(
        "mov cr3, rdi",
        "add rsp, rsi",
        "lea rax, [rip + {main}]",
        "add rax, rsi",
        "jmp rax",

        main = sym main,
    )
}

unsafe extern "sysv64" fn main() -> ! {
    let boot_info = boot_info();
    #[cfg(debug_assertions)]
    pause();

    /*let mut display_writer = DisplayWriter {
        graphics_info: &boot_info.graphics_info,
        line: 0,
        col: 0,
    };

    writeln!(&mut display_writer, "Hello World from the Kernel, calling at address {:p}!!!!", main as *const ()).unwrap();

    let regions = boot_info.memory_regions();
    writeln!(&mut display_writer, "Found {} usable physical memory regions!", regions.len()).unwrap();
    for region in regions {
        writeln!(
            &mut display_writer,
            "Usable physical memory in {}..{}, {} pages!",
            region.base,
            region.base.byte_add(region.pages * PAGE_SIZE),
            region.pages,
        ).unwrap();
    }*/

    //let [virt_start, virt_end] = boot_info().v_addr_range();
    //writeln!(&mut display_writer, "Higher-half virtual addressing available in range {virt_start}..{virt_end}").unwrap();

    loop {}
}