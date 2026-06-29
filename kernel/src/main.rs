#![no_std]
#![no_main]

use core::{
    arch::{asm, naked_asm},
    panic::PanicInfo,
};

use elpytios_bootinfo::{BootInfo, PAGE_SIZE};
use elpytios_kernel::{
    println,
    serial::{Com, serial_init},
    vaddr::{VAddr, VirtualMapBuilder},
};

#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
    println!("{info}");
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

const HIGHER_HALF_ADDRESS_BASE: VAddr = VAddr::new(0xffffffff80000000);

unsafe extern "sysv64" fn setup(info: &'static BootInfo) -> ! {
    unsafe {
        serial_init(Com::Com3);
    }

    println!("Setting up kernel...");

    for reg in &info.memory_regions {
        println!("{:p}, {} pages", reg.base, reg.pages);
    }

    let max = info.page_table_init_len;
    let mut i = 0;
    let mut virtual_map = unsafe {
        VirtualMapBuilder::new(
            510,
            || {
                (i < max).then(|| {
                    println!("yo");
                    i += 1;
                    info.page_table_init.byte_add((i - 1) * PAGE_SIZE)
                })
            },
            |p_addr| p_addr.addr() as *mut (),
        )
    };

    println!(
        "Reserving {} pages in {:p} for initial virtual mapping...",
        info.page_table_init_len, info.page_table_init
    );

    /*for i in 0..info.kernel_pages {
        /*println!(
            "{:p} -> {:p}",
            info.kernel_base.byte_add(i * PAGE_SIZE),
            HIGHER_HALF_ADDRESS_BASE.byte_add(i * PAGE_SIZE),
        );
        virtual_map.map(
            info.kernel_base.byte_add(i * PAGE_SIZE),
            HIGHER_HALF_ADDRESS_BASE.byte_add(i * PAGE_SIZE),
            VFlags::WRITABLE,
        ).unwrap_or_else(|e| panic!("{e}"));*/
    }*/

    //virtual_map.finish();
    //let (virtual_map_addr, virtual_map) = virtual_map.finish().unwrap_or_else(|e| panic!("{e}"));

    /*let mut alloc = None;
    for region in boot_info().memory_regions() {
        if !matches!(region.reclaim, MemoryReclaimType::Free) { continue }

        let (alloc, (mut base, mut pages)) = match alloc.as_mut() {
            Some(alloc) => (alloc, (region.base, region.pages)),
            None if region.pages >= 1 => {
                let ptr = region.base.addr() as *mut PhysicalPageAllocator;
                (
                    unsafe {
                        ptr.write(PhysicalPageAllocator::new());
                        alloc.insert(ptr.as_mut_unchecked())
                    },
                    (
                        region.base.byte_add(PAGE_SIZE),
                        region.pages - 1,
                    )
                )
            }
            None => continue,
        };

        while pages > 0 {
            let Ok(layout) = AllocTree::layout(pages) else { break };
            let total_page_count = layout.size().div_ceil(PAGE_SIZE) + layout.node_count();

            if total_page_count > pages {
                pages /= 2;
            } else {
                unsafe {
                    let tree = AllocTree::new(base.addr() as *mut (), layout);
                    alloc.push_tree(base, tree);
                }

                base = base.byte_add(total_page_count * PAGE_SIZE);
                pages -= total_page_count;
            }
        }
    }*/

    println!("Enabling virtual paging...");
    unsafe {
        asm!(
            "lea rax, [rip + {main}]",
            "jmp rax",

            main = sym main,

            options(noreturn),
        )
    }
}

unsafe extern "sysv64" fn main() -> ! {
    #[cfg(debug_assertions)]
    pause();

    println!("Hello, world!");

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

/*#[unsafe(naked)]
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
}*/
