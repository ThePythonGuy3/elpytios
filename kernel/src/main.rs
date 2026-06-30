#![no_std]
#![no_main]

use core::{
    arch::{asm, naked_asm},
    fmt::Write,
    panic::PanicInfo,
};

use elpytios_bootinfo::{BootInfo, IdentityMapFlags, MemoryRegion, PAGE_SIZE, Reloc, paddr::PAddr};
use elpytios_elf::sys::{ElfRela64, ElfRela64Type};
use elpytios_kernel::{
    alloc::{AllocTree, PhysicalPageAllocator},
    serial::{Com, Serial, serial_init},
    statics::{get_virtual_map, set_virtual_map},
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
        "cli",
        "cld",
        "lea rax, [rip + {setup_identity_mapped}]",
        "jmp rax",

        setup_identity_mapped = sym setup_identity_mapped,
    )
}

const HIGHER_HALF_ADDRESS_BASE: VAddr = VAddr::new(0xffff_8000_0000_0000);

#[inline]
fn reserved_pages_allocator(info: &BootInfo, used_pages: &mut usize) -> impl FnMut() -> Option<PAddr> {
    || {
        (*used_pages < info.page_table_init_len).then(|| {
            let ptr = info.page_table_init.byte_add(*used_pages * PAGE_SIZE);
            *used_pages += 1;

            debug_assert_eq!(ptr.addr() % PAGE_SIZE, 0, "Page table pointer isn't page-aligned");
            ptr
        })
    }
}

struct SerialLogger(Com);
impl log::Log for SerialLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            _ = match (record.file(), record.line()) {
                (Some(file), Some(line)) => writeln!(Serial(self.0), "[{}]\t{}:{}\t> {}", record.level(), file, line, record.args()),
                _ => writeln!(Serial(self.0), "[{}]\t{}\t> {}", record.level(), record.target(), record.args()),
            }
        }
    }

    fn flush(&self) {}
}

static LOGGER: SerialLogger = SerialLogger(Com::Com3);

unsafe extern "sysv64" fn setup_identity_mapped(info: &'static BootInfo) -> ! {
    // Notes:
    // - `log` mustn't be setup here; wait until symbols are relocated

    let v_slide = HIGHER_HALF_ADDRESS_BASE
        .addr()
        .checked_sub(info.kernel_base.addr())
        .expect("Kernel physical address somehow higher than higher-half addressing base");

    let mut used_pages = 0;
    let mut virtual_map = unsafe { VirtualMapBuilder::new(510, reserved_pages_allocator(info, &mut used_pages), |p_addr| p_addr.addr() as *mut ()) };

    let mut max_virt = usize::MIN;
    for map in &info.identity_maps {
        for i in 0..map.region.pages {
            let phys = map.region.base.byte_add(i * PAGE_SIZE).addr();
            let virt = phys + v_slide;
            max_virt = max_virt.max(virt);

            virtual_map
                .map(PAddr::new(phys), VAddr::new(phys), {
                    let mut flags = VFlags::GLOBAL;
                    if map.flags.contains(IdentityMapFlags::WRITABLE) {
                        flags |= VFlags::WRITABLE
                    }
                    flags
                })
                .expect("Couldn't identity map kernel segment");

            virtual_map
                .map(PAddr::new(phys), VAddr::new(virt), {
                    let mut flags = VFlags::GLOBAL;
                    if map.flags.contains(IdentityMapFlags::WRITABLE) {
                        flags |= VFlags::WRITABLE
                    }
                    flags
                })
                .expect("Couldn't virtual map kernel segment");
        }
    }

    let (page_table_phys, virtual_map) = virtual_map.finish().expect("Couldn't build virtual map table");
    let setup_virtual_mapped = (setup_virtual_mapped as *const ())
        .addr()
        .checked_add(v_slide)
        .expect("`setup_virtual_mapped()` virtual address overflowed");

    unsafe {
        set_virtual_map(virtual_map);

        asm!(
            "mov cr3, {page_table_phys}",
            "add rsp, {v_slide}",
            "and rsp, -16",
            "call {setup_virtual_mapped}",

            page_table_phys = in(reg) page_table_phys.addr(),
            v_slide = in(reg) v_slide,
            setup_virtual_mapped = in(reg) setup_virtual_mapped,
            in("rdi") (info as *const BootInfo).byte_add(v_slide).as_ref_unchecked(),
            in("rsi") max_virt + PAGE_SIZE,
            in("rdx") used_pages,

            options(noreturn),
        )
    }
}

unsafe extern "sysv64" fn setup_virtual_mapped(info: &'static BootInfo, mut next_v_addr: VAddr, mut used_pages: usize) -> ! {
    // Relocate all symbols to higher-half addressing
    // Identity-mapping is still present at this point, so it is okay to cast `PAddr` into pointers
    // Identity-mapping will be gone after this scope is exited
    // TODO ^ do just that
    {
        let kernel_ptr = info.kernel_elf_base.addr() as *mut u8;
        let v_slide = HIGHER_HALF_ADDRESS_BASE
            .addr()
            .checked_sub(info.kernel_base.addr())
            .expect("Kernel physical address somehow higher than higher-half addressing base")
            .cast_signed() as i64;

        for &Reloc { offset, size, stride } in &info.relocations {
            for i in 0..size / stride {
                unsafe {
                    let rela = kernel_ptr
                        .cast::<ElfRela64>()
                        .byte_add(offset - info.kernel_virt_base)
                        .add(i)
                        .read_unaligned();

                    match rela.info.kind {
                        ElfRela64Type::X86_64_RELATIVE => {
                            let slide = v_slide + kernel_ptr.addr() as i64 - info.kernel_virt_base as i64;
                            let patch_addr = kernel_ptr.add(rela.offset as usize - info.kernel_virt_base);
                            let value = slide + rela.addend;
                            patch_addr.cast::<i64>().write(value);
                        }
                        kind => panic!("Unsupported Elf64_Rela kind: {}", kind.0),
                    }
                }
            }
        }
    }

    // Setup `log` here, symbols have been relocated
    unsafe {
        serial_init(Com::Com3);
        log::set_logger_racy(&LOGGER).expect("Log already setup before symbol relocations");
        log::set_max_level_racy(cfg_select! {
            debug_assertions => log::LevelFilter::Trace,
            not(debug_assertions) => log::LevelFilter::Info,
        });
    }

    // When running through `x qemu run --debug`, wait until a corresponding GDB client executes this:
    //
    //     set language c
    //     set *(unsigned char*)&__DEBUG_HALT = 0
    //     set language rust
    //     continue
    //
    // This is to ensure the kernel has been loaded to memory at offset 0xffff_8000_0000_0000 before
    // inserting software breakpoints and looking up symbosl at the same offset
    #[cfg(debug_assertions)]
    {
        use log::debug;

        #[unsafe(no_mangle)]
        #[used]
        static mut __DEBUG_HALT: u8 = 1;

        debug!("Waiting for debugger...");
        while unsafe { (&raw const __DEBUG_HALT).read_volatile() } != 0 {
            core::hint::spin_loop();
        }

        debug!("Continuing!");
    }

    info!(
        "Setting up physical page allocator: found {} usable memory regions",
        info.memory_regions.len()
    );

    let v_map = get_virtual_map();

    // Setup global physical page allocator
    // TODO ^ do just that
    {
        let mut alloc = PhysicalPageAllocator::new();
        for &MemoryRegion { mut base, mut pages } in &info.memory_regions {
            if base.addr() == 0 {
                base = base.byte_add(PAGE_SIZE);
                pages -= 1;
            }

            while pages > 1 {
                let mut taken_pages = pages;
                loop {
                    let layout = AllocTree::layout(taken_pages).expect("`AllocTree` layout error");
                    let meta_pages = layout.size().div_ceil(PAGE_SIZE);
                    let try_take = meta_pages + layout.node_count();

                    if try_take > pages {
                        taken_pages /= 2;
                    } else {
                        unsafe {
                            for i in 0..meta_pages {
                                let offset = i * PAGE_SIZE;
                                v_map
                                    .map(
                                        base.byte_add(offset),
                                        next_v_addr.byte_add(offset),
                                        VFlags::GLOBAL | VFlags::WRITABLE,
                                        reserved_pages_allocator(info, &mut used_pages),
                                    )
                                    .expect("Couldn't virtual map alloc tree");
                            }

                            let tree = AllocTree::new(next_v_addr.ptr_mut(), layout);
                            alloc.push_tree(base.byte_add(meta_pages * PAGE_SIZE), tree);

                            next_v_addr = next_v_addr.byte_add(meta_pages * PAGE_SIZE);
                        }

                        base = base.byte_add(try_take * PAGE_SIZE);
                        pages -= try_take;
                        break
                    }
                }
            }
        }
    }

    unsafe { main() }
}

unsafe extern "sysv64" fn main() -> ! {
    info!("Hello, world! Kernel is now in higher-half addressing!");

    loop {}
}
