#![no_std]
#![no_main]

use core::{
    arch::{asm, naked_asm},
    fmt::Write,
    panic::PanicInfo,
};

use elpytios_bootinfo::{BootInfo, IdentityMapFlags, PAGE_SIZE, paddr::PAddr};
use elpytios_kernel::{
    serial::{Com, Serial, serial_init},
    vaddr::{VAddr, VFlags, VirtualMapBuilder},
};
use log::{error, info};

#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
    error!("{info}");
    loop {}
}

#[unsafe(naked)]
#[unsafe(export_name = "_start")]
unsafe extern "sysv64" fn jump_from_bootloader(info: &'static BootInfo) -> ! {
    naked_asm!(
        "lea rax, [rip + {setup}]",
        "jmp rax",

        setup = sym setup,
    )
}

const HIGHER_HALF_ADDRESS_BASE: VAddr = VAddr::new(0xffff_8000_0000_0000);

unsafe extern "sysv64" fn setup(info: &'static BootInfo) -> ! {
    unsafe {
        serial_init(Com::Com3);
        _ = log::set_logger_racy(&SerialLogger(Com::Com3));
        log::set_max_level_racy(match cfg!(debug_assertions) {
            false => log::LevelFilter::Info,
            true => log::LevelFilter::Trace,
        });

        struct SerialLogger(Com);
        impl log::Log for SerialLogger {
            fn enabled(&self, _metadata: &log::Metadata) -> bool {
                true
            }

            fn log(&self, record: &log::Record) {
                if self.enabled(record.metadata()) {
                    _ = match (record.file(), record.line()) {
                        (Some(file), Some(line)) => writeln!(Serial(self.0), "[{}] {}:{}\t- {}", record.level(), file, line, record.args()),
                        _ => writeln!(Serial(self.0), "[{}] {}\t- {}", record.level(), record.target(), record.args()),
                    }
                }
            }

            fn flush(&self) {}
        }
    }

    info!("Setting up kernel, loaded at {:p}", info.kernel_base);
    let v_slide = HIGHER_HALF_ADDRESS_BASE
        .addr()
        .checked_sub(info.kernel_base.addr())
        .expect("Kernel physical address somehow higher than higher-half addressing base");

    let mut i = 0;
    let mut virtual_map = unsafe {
        VirtualMapBuilder::new(
            510,
            || {
                (i < info.page_table_init_len).then(|| {
                    let ptr = info.page_table_init.byte_add(i * PAGE_SIZE);
                    i += 1;

                    debug_assert_eq!(ptr.addr() % PAGE_SIZE, 0, "Page table pointer isn't page-aligned");
                    ptr
                })
            },
            |p_addr| p_addr.addr() as *mut (),
        )
    };

    info!("Offset by 0x{v_slide:x} for higher-half addressing");
    info!(
        "Found {} reserved pages in {:p} for initial virtual mapping",
        info.page_table_init_len, info.page_table_init
    );

    for map in &info.identity_maps {
        for i in 0..map.region.pages {
            let phys = map.region.base.byte_add(i * PAGE_SIZE).addr();
            let virt = phys + v_slide;

            virtual_map
                .map(PAddr::new(phys), VAddr::new(phys), {
                    let mut flags = VFlags::GLOBAL;
                    if map.flags.contains(IdentityMapFlags::WRITABLE) {
                        flags |= VFlags::WRITABLE
                    }
                    flags
                })
                .expect("Couldn't identity map");

            virtual_map
                .map(PAddr::new(phys), VAddr::new(virt), {
                    let mut flags = VFlags::GLOBAL;
                    if map.flags.contains(IdentityMapFlags::WRITABLE) {
                        flags |= VFlags::WRITABLE
                    }
                    flags
                })
                .expect("Couldn't identity map");
        }
    }

    let (page_table_phys, virtual_map) = virtual_map.finish().expect("Couldn't build virtual map table");
    let main = (main as *const ())
        .addr()
        .checked_add(v_slide)
        .expect("`main(..)` virtual address overflowed");

    unsafe {
        asm!(
            "mov cr3, {page_table_phys}",
            "add rsp, {v_slide}",
            "and rsp, -16",
            "call {main}",
            "ud2",

            page_table_phys = in(reg) page_table_phys.addr(),
            v_slide = in(reg) v_slide,
            main = in(reg) main,
            in("rdi") info,

            options(noreturn),
        )
    }
}

#[unsafe(export_name = "_main")]
unsafe extern "sysv64" fn main(_info: &'static BootInfo) -> ! {
    #[cfg(debug_assertions)]
    {
        #[unsafe(no_mangle)]
        #[used]
        static mut __DEBUG_HALT: u8 = 1;

        while unsafe { (&raw const __DEBUG_HALT).read_volatile() } != 0 {
            core::hint::spin_loop();
        }
    }

    info!("Hello, world! Kernel is now in higher-half addressing!");

    loop {}
}
