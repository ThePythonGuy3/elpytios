#![forbid(unfulfilled_lint_expectations)]
#![feature(core_float_math)]
#![no_std]
#![no_main]

use core::{
    arch::{asm, naked_asm},
    fmt::Write,
    iter::once,
    panic::PanicInfo,
};

use elpytios_bootinfo::{BootInfo, IdentityMapFlags, MemoryRegion, PAGE_SIZE, Reloc, paddr::PAddr};
use elpytios_elf::sys::{ElfRela64, ElfRela64Type};
use elpytios_kernel::{
    allocator::{AllocTree, PhysicalPageAllocator},
    framebuffer::FrameBuffer,
    serial::{Com, Serial, serial_init},
    statics::{get_phys_alloc, get_virtual_map, phys_to_virt, set_direct_map_offset, set_frame_buffer, set_phys_alloc, set_virtual_map},
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
        "jmp {setup_identity_mapped}",

        setup_identity_mapped = sym setup_identity_mapped,
    )
}

const HIGHER_HALF_ADDRESS_BASE: VAddr = VAddr::new(0xffff_8000_0000_0000);

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

struct MemoryRegions<'a> {
    available: &'a [MemoryRegion],
    head: MemoryRegion,
}

impl<'a> MemoryRegions<'a> {
    fn new(source: &'a [MemoryRegion]) -> Self {
        let &[ref available @ .., head] = source else { panic!("Not enough memory to start the kernel") };
        Self { available, head }
    }

    fn take_head(&mut self) -> Option<PAddr> {
        let head = loop {
            match self.head.pages {
                0 => {
                    let &[ref available @ .., head] = self.available else { return None };
                    self.available = available;
                    self.head = head;
                }
                n => {
                    self.head.pages = n - 1;
                    break self.head.base.byte_add((n - 1) * PAGE_SIZE)
                }
            }
        };
        Some(head)
    }
}

/// # Safety
/// - Available memory regions must *not* include the kernel code, stack, and boot info itself;
///   i.e., they must be usable immediately.
/// - Any references must point to the defined custom `MEM_*` memory types in the bootloader.
/// - See safety notes of [`setup_virtual_mapped`].
unsafe extern "sysv64" fn setup_identity_mapped(info: &'static BootInfo) -> ! {
    // Notes:
    // - `log` mustn't be setup here; wait until symbols are relocated

    let mut regions = MemoryRegions::new(&info.memory_regions);
    let v_slide = HIGHER_HALF_ADDRESS_BASE
        .addr()
        .checked_sub(info.kernel_base.addr())
        .expect("Kernel physical address somehow higher than higher-half addressing base");

    let mut v_map = unsafe {
        VirtualMapBuilder::new(
            || regions.take_head().inspect(|addr| (addr.addr() as *mut u8).write_bytes(0, PAGE_SIZE)),
            |p_addr| p_addr.addr() as *mut (),
        )
    };

    let mut direct_map_offset = usize::MIN;
    for map in &info.identity_maps {
        let mut flags = VFlags::empty();
        if map.flags.contains(IdentityMapFlags::WRITABLE) {
            flags |= VFlags::WRITABLE;
        } else {
            flags |= VFlags::GLOBAL;
        }
        if !map.flags.contains(IdentityMapFlags::EXECUTABLE) {
            flags |= VFlags::EXECUTE_DISABLE;
        }

        v_map
            .map(map.region.base, VAddr::new(map.region.base.addr()), map.region.pages, flags)
            .expect("Couldn't identity map kernel segment");

        v_map
            .map(map.region.base, VAddr::new(map.region.base.addr() + v_slide), map.region.pages, flags)
            .expect("Couldn't virtual map kernel segment");

        direct_map_offset = direct_map_offset.max(map.region.base.addr() + v_slide + map.region.pages * PAGE_SIZE);
    }

    // Direct map *all* of RAM to the specified direct-map offset
    let direct_map_offset = direct_map_offset.next_multiple_of(2 << 30); // Align to a gigabyte
    unsafe { set_direct_map_offset(direct_map_offset) }

    for region in &info.memory_regions {
        v_map
            .map(
                region.base,
                VAddr::new(region.base.addr() + direct_map_offset),
                region.pages,
                VFlags::WRITABLE,
            )
            .unwrap();
    }

    let (page_table_phys, virtual_map) = unsafe { v_map.finish() }.expect("Couldn't build virtual map table");
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
            "jmp {setup_virtual_mapped}",

            page_table_phys = in(reg) page_table_phys.addr(),
            v_slide = in(reg) v_slide,
            setup_virtual_mapped = in(reg) setup_virtual_mapped,
            in("rdi") (info as *const BootInfo).byte_add(v_slide).as_ref_unchecked(),
            in("rsi") &regions,

            options(noreturn),
        )
    }
}

unsafe extern "sysv64" fn setup_virtual_mapped(info: &'static BootInfo, regions: &MemoryRegions) -> ! {
    // Relocate all symbols to higher-half addressing
    // Identity-mapping is still present at this point, so it is okay to cast `PAddr` into pointers
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

    // Setup global physical page allocator
    {
        info!(
            "Setting up physical page allocator: found {} usable memory regions",
            info.memory_regions.len()
        );

        let mut phys_alloc = PhysicalPageAllocator::new();
        for MemoryRegion { mut base, mut pages } in regions.available.iter().copied().chain(once(regions.head)) {
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
                            let tree = AllocTree::new(phys_to_virt(base).ptr_mut(), layout);
                            phys_alloc.push_tree(base.byte_add(meta_pages * PAGE_SIZE), tree);
                        }

                        base = base.byte_add(try_take * PAGE_SIZE);
                        pages -= try_take;
                        break
                    }
                }
            }
        }

        unsafe { set_phys_alloc(phys_alloc) }
    }

    // Virtual-map the framebuffer
    {
        let v_map = get_virtual_map();
        let phys_alloc = &mut *get_phys_alloc().lock();

        let fb_phys = info.graphics_info.frame_buffer.addr();
        let fb_size = info.graphics_info.frame_buffer_size;

        let fb_phys_base = fb_phys & !(PAGE_SIZE - 1);
        let fb_phys_end = (fb_phys + fb_size).next_multiple_of(PAGE_SIZE);
        let fb_page_count = (fb_phys_end - fb_phys_base) / PAGE_SIZE;

        unsafe {
            let p_addr = PAddr::new(fb_phys_base);
            v_map
                .map(
                    p_addr,
                    phys_to_virt(p_addr),
                    fb_page_count,
                    VFlags::GLOBAL | VFlags::WRITABLE | VFlags::WRITE_THROUGH,
                    || {
                        phys_alloc
                            .alloc(1)
                            .ok()
                            .map(|id| id.addr())
                            .inspect(|&addr| phys_to_virt(addr).ptr_mut::<u8>().write_bytes(0, PAGE_SIZE))
                    },
                )
                .unwrap();
        }

        unsafe {
            set_frame_buffer(FrameBuffer {
                width: info.graphics_info.w,
                height: info.graphics_info.h,
                stride: info.graphics_info.stride,
                format: info.graphics_info.pixel_format,
                pointer: phys_to_virt(PAddr::new(fb_phys)).ptr_mut(),
            })
        }
    }

    unsafe { main() }
}

/// # Safety
/// - All [`statics`](elpytios_kernel::statics) must have been initialized prior to calling this
///   function.
unsafe extern "sysv64" fn main() -> ! {
    info!("Hello, world! Kernel is now in higher-half addressing!");

    {
        use elpytios_bootinfo::PixelFormat;
        use elpytios_kernel::statics::get_frame_buffer;

        let fbo = get_frame_buffer();
        match fbo.format {
            fmt @ (PixelFormat::RGB_8_BIT | PixelFormat::BGR_8_BIT) => {
                let invert_br = matches!(fmt, PixelFormat::BGR_8_BIT);
                for y in 0..fbo.height {
                    for x in 0..fbo.width {
                        let fx = x as f32 / (fbo.width - 1) as f32;
                        let fy = y as f32 / (fbo.height - 1) as f32;

                        let r = (fx * 255.) as u8;
                        let g = (fy * 255.) as u8;
                        let b = (core::f32::math::sqrt((fx * 2. - 1.).abs() * (fy * 2. - 1.).abs()) * 255.) as u8;
                        let a = 255;

                        unsafe {
                            fbo.pointer.cast::<[u8; 4]>().add(y * fbo.stride + x).write_volatile(match invert_br {
                                false => [r, g, b, a],
                                true => [b, g, r, a],
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    loop {}
}
