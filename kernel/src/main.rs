#![feature(custom_inner_attributes)]
#![rustfmt::skip]

#![no_std]
#![no_main]

use core::{arch::{asm, naked_asm}, fmt::Write, panic::PanicInfo};

use elpytios_bootinfo::{MemoryRegion, PAGE_SIZE, paddr::PAddr};
use elpytios_kernel::{alloc::{AllocTree, PhysicalPageAllocator}, boot_info, rendering::DisplayWriter};

#[panic_handler]
fn hanic_pandler(_info: &PanicInfo) -> ! {
    loop {}
}

#[unsafe(naked)]
#[unsafe(export_name = "_start")]
unsafe extern "sysv64" fn jump_from_bootloader() -> ! {
    naked_asm!(
        "lea rax, [rip + {setup_virtual_paging}]",
        "jmp rax",

        setup_virtual_paging = sym setup_virtual_paging,
    )
}

unsafe extern "sysv64" fn setup_virtual_paging() -> ! {
    let mut alloc = None;
    for region in boot_info().memory_regions() {
        let (alloc, MemoryRegion { mut base, mut pages }) = match alloc.as_mut() {
            Some(alloc) => (alloc, *region),
            None if region.pages >= 1 => {
                let ptr = region.base.addr() as *mut PhysicalPageAllocator;
                (
                    unsafe {
                        ptr.write(PhysicalPageAllocator::new());
                        alloc.insert(ptr.as_mut_unchecked())
                    },
                    MemoryRegion {
                        base: region.base.byte_add(PAGE_SIZE),
                        pages: region.pages - 1,
                    }
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
    }

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