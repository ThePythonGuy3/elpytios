#![no_std]
#![no_main]

use core::{
    arch::{asm, naked_asm},
    panic::PanicInfo,
};

use elpytios_bootinfo::{BootInfo, IdentityMapFlags, PAGE_SIZE};
use elpytios_kernel::{
    println,
    serial::{Com, serial_init},
    vaddr::{VAddr, VFlags, VirtualMapBuilder},
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

    println!("hey {:p} hoy", info as *const _);
    println!("Setting up kernel, loaded at {:p}", info.kernel_base);
    let v_slide = HIGHER_HALF_ADDRESS_BASE
        .addr()
        .checked_sub(info.kernel_base.addr())
        .expect("Kernel physical address somehow higher than higher-half addressing base");

    println!("page table at {:?}", info.page_table_init);
    println!("page table count {}", info.page_table_init_len);

    let mut i = 0;
    let mut virtual_map = unsafe {
        VirtualMapBuilder::new(
            510,
            || {
                (i < info.page_table_init_len).then(|| {
                    let ptr = info.page_table_init.byte_add(i * PAGE_SIZE);
                    i += 1;

                    debug_assert_eq!(ptr.addr() % PAGE_SIZE, 0, "Page table pointer isn't page-aligned");
                    //println!("    ptr          = {:p}", ptr);
                    //println!("    ptr addr     = {:#x}", ptr.addr());
                    //println!("    ptr addr end = {:#x}", ptr.addr() + PAGE_SIZE - 1);

                    //volatile_set_memory(ptr.addr() as *mut u8, 0, PAGE_SIZE);
                    //println!("    bytes written!");
                    ptr
                })
            },
            |p_addr| p_addr.addr() as *mut (),
        )
    };

    println!("Offset by 0x{v_slide:x} for higher-half addressing");
    println!(
        "Found {} reserved pages in {:p} for initial virtual mapping",
        info.page_table_init_len, info.page_table_init
    );

    let mut free_virt_addr = HIGHER_HALF_ADDRESS_BASE;
    for map in &info.identity_maps {
        for i in 0..map.region.pages {
            virtual_map
                .map(
                    map.region.base.byte_add(i * PAGE_SIZE),
                    VAddr::new(map.region.base.byte_add(i * PAGE_SIZE).addr()),
                    {
                        let mut flags = VFlags::GLOBAL;
                        if map.flags.contains(IdentityMapFlags::WRITABLE) {
                            flags |= VFlags::WRITABLE
                        }
                        flags
                    },
                )
                .expect("Couldn't virtual-map");
        }
    }

    let (page_table_phys, virtual_map) = virtual_map.finish().expect("Couldn't build virtual map table");

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

    unsafe {
        asm!(
            "mov rdi, {info}",
            "mov rsi, {free_virt_addr}",
            "lea rax, [rip + {main}]",
            "jmp rax",

            info = in(reg) info,
            free_virt_addr = in(reg) free_virt_addr.addr(),
            main = sym main,

            options(noreturn),
        )
    }
}

unsafe extern "sysv64" fn main(info: &'static BootInfo, free_virt_addr: VAddr) -> ! {
    //#[cfg(debug_assertions)]
    //pause();

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
