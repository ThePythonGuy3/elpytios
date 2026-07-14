#![forbid(unfulfilled_lint_expectations)]
#![feature(core_float_math, ptr_alignment_type)]
#![no_std]
#![no_main]

extern crate alloc;
use core::{
    arch::{asm, naked_asm},
    cell::RefCell,
    fmt::Write,
    iter::once,
    panic::PanicInfo,
};

use elpytios_bootinfo::{BootInfo, IdentityMapFlags, MemoryRegion, PAGE_SIZE, Reloc, paddr::PAddr};
use elpytios_elf::sys::{ElfRela64, ElfRela64Type};
use elpytios_kernel::{
    ScratchPages,
    allocator::{AllocTree, PHYS_ALLOC_ALIGNMENT, PhysicalPageAllocator},
    device::{CpuContext, init_device_tree},
    framebuffer::FrameBuffer,
    serial::{Com, Serial, serial_init},
    statics::{get_virtual_map, phys_to_virt, set_direct_map_offset, set_frame_buffer, set_phys_alloc, set_virtual_map},
    vaddr::{VAddr, VFlags, VirtualMapBuilder},
};
use log::{debug, error, info};

#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
    error!("{info}");
    loop {}
}

#[unsafe(naked)]
#[unsafe(export_name = "_start")] // Tell the linker that this is our entry point
unsafe extern "sysv64" fn jump_from_bootloader(info: &'static BootInfo) -> ! {
    naked_asm!(
        // Clear interrupt handlers, will be reinitialized by `setup_virtual_mapped()`
        "cli",
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

    let regions = RefCell::new(MemoryRegions::new(&info.memory_regions));
    let mut scratch_pages = ScratchPages { pages: &info.scratch_pages };

    let kernel_base = info
        .identity_maps
        .iter()
        .min_by_key(|map| map.region.base)
        .expect("Didn't find any identity maps")
        .region
        .base;
    let v_slide = HIGHER_HALF_ADDRESS_BASE
        .addr()
        .checked_sub(kernel_base.addr())
        .expect("Kernel physical address somehow higher than higher-half addressing base");

    let page_table_phys = scratch_pages.take().expect("Not enough scratch pages for page table");
    let setup_virtual_mapped = {
        let mut v_map = unsafe { VirtualMapBuilder::new(page_table_phys, || regions.borrow_mut().take_head(), |p_addr| p_addr.addr() as *mut ()) };

        let mut direct_map_offset = usize::MIN;
        for map in &info.identity_maps {
            let mut flags = VFlags::GLOBAL;
            if map.flags.contains(IdentityMapFlags::WRITABLE) {
                flags |= VFlags::WRITABLE;
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

        v_map
            .map(
                page_table_phys,
                VAddr::new(page_table_phys.addr() + direct_map_offset),
                1,
                VFlags::GLOBAL | VFlags::WRITABLE | VFlags::EXECUTE_DISABLE,
            )
            .unwrap();

        for region in &info.memory_regions {
            v_map
                .map(
                    region.base,
                    VAddr::new(region.base.addr() + direct_map_offset),
                    region.pages,
                    VFlags::GLOBAL | VFlags::WRITABLE | VFlags::EXECUTE_DISABLE,
                )
                .unwrap();
        }

        let virtual_map = unsafe { v_map.finish() };
        let setup_virtual_mapped = (setup_virtual_mapped as *const ())
            .addr()
            .checked_add(v_slide)
            .expect("`setup_virtual_mapped()` virtual address overflowed");

        unsafe { set_virtual_map(virtual_map) }
        setup_virtual_mapped
    };

    unsafe {
        let tmp = 0usize;
        asm!(
            // Enable `GLOBAL` mapping, i.e. pages in TLB that don't get flushed
            "mov {tmp}, cr4",
            "or {tmp}, 1 << 7",
            "mov cr4, {tmp}",

            "mov cr3, {page_table_phys}",
            "add rsp, {v_slide}",
            "and rsp, -16",
            "jmp {setup_virtual_mapped}",

            tmp = in(reg) tmp,
            page_table_phys = in(reg) page_table_phys.addr(),
            v_slide = in(reg) v_slide,
            setup_virtual_mapped = in(reg) setup_virtual_mapped,
            in("rdi") (info as *const BootInfo).byte_add(v_slide).as_ref_unchecked(),
            in("rsi") &regions.into_inner(),
            in("rdx") &mut scratch_pages,
            in("rcx") kernel_base.addr(),

            options(noreturn),
        )
    }
}

unsafe extern "sysv64" fn setup_virtual_mapped(
    info: &'static BootInfo,
    regions: &MemoryRegions,
    scratch_pages: &mut ScratchPages,
    kernel_base: PAddr,
) -> ! {
    // Relocate all symbols to higher-half addressing
    // Identity-mapping is still present at this point, so it is okay to cast `PAddr` into pointers
    let kernel_ptr = info.kernel_elf_base;
    let v_slide = HIGHER_HALF_ADDRESS_BASE
        .addr()
        .checked_sub(kernel_base.addr())
        .expect("Kernel physical address somehow higher than higher-half addressing base")
        .cast_signed() as i64;

    for &Reloc { offset, size, stride } in &info.relocations {
        for i in 0..size / stride {
            unsafe {
                let rela = (kernel_ptr.addr() as *mut u8)
                    .cast::<ElfRela64>()
                    .byte_add(offset - info.kernel_virt_base)
                    .add(i)
                    .read_unaligned();

                match rela.info.kind {
                    ElfRela64Type::X86_64_RELATIVE => {
                        let slide = v_slide + kernel_ptr.addr() as i64 - info.kernel_virt_base as i64;
                        let patch_addr = (kernel_ptr.addr() as *mut u8).add(rela.offset as usize - info.kernel_virt_base);
                        let value = slide + rela.addend;
                        patch_addr.cast::<i64>().write(value);
                    }
                    kind => panic!("Unsupported Elf64_Rela kind: {}", kind.0),
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

        debug!(
            "Setting up kernel at {kernel_ptr:p} -> {:p}",
            VAddr::new(kernel_ptr.addr().wrapping_add_signed(v_slide as isize))
        );
    }

    // When running through `x qemu run --debug`, wait until a corresponding GDB client executes this:
    //
    //     target remote [host, usuallty `localhost`]:[port, usually `1234`]
    //     add-symbol-file [path/to]/elpytios-kernel -o [offset; see "Setting up kernel at ..." log]
    //
    //     set language c
    //     set *(unsigned char*)&__DEBUG_HALT = 0
    //     set language rust
    //     continue
    //
    // This is to ensure the kernel has been loaded to memory at higher-half address before inserting
    // software breakpoints and looking up symbols at the same offset
    #[cfg(debug_assertions)]
    {
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
            "Initializing physical page allocator: found {} usable memory regions",
            info.memory_regions.len()
        );

        let mut phys_alloc = unsafe { PhysicalPageAllocator::new() };
        for MemoryRegion { base, pages } in regions.available.iter().copied().chain(once(regions.head)) {
            debug!(
                "\tAvailable memory region found at [{base:p}..{:p}], {pages} pages",
                base.byte_add(pages * PAGE_SIZE)
            );

            let usable_start = base.addr();
            let mut usable_end = usable_start + pages * PAGE_SIZE;

            // Trees need to be aligned to `PHYS_ALLOC_ALIGNMENT`, so round down and manually fill "ghost" pages
            let mut tree_start = usable_start & !(PHYS_ALLOC_ALIGNMENT.as_usize() - 1);
            let mut tree_end = usable_end & !(PHYS_ALLOC_ALIGNMENT.as_usize() - 1);

            while tree_start < tree_end {
                let layout = AllocTree::layout((tree_end - tree_start) / PAGE_SIZE).expect("`AllocTree` layout error");
                let meta_pages = layout.size().div_ceil(PAGE_SIZE);
                let tree_end_actual = tree_start + layout.node_count() * PAGE_SIZE;

                let (tree, ghost_pages) = if (usable_end - tree_end_actual) / PAGE_SIZE >= meta_pages {
                    // Store tree metadata to the right, outside the tree
                    usable_end -= meta_pages * PAGE_SIZE;
                    unsafe {
                        (
                            AllocTree::new(phys_to_virt(PAddr::new(usable_end)).ptr_mut(), layout),
                            usable_start
                                .checked_sub(tree_start)
                                .and_then(|ghost_size| (ghost_size != 0).then_some(ghost_size / PAGE_SIZE)),
                        )
                    }
                } else if (tree_end_actual - usable_start) / PAGE_SIZE >= meta_pages {
                    // Store tree metadata to the left, adding along ghost pages
                    unsafe {
                        (
                            AllocTree::new(phys_to_virt(PAddr::new(usable_start)).ptr_mut(), layout),
                            Some((usable_start + meta_pages * PAGE_SIZE - tree_start) / PAGE_SIZE),
                        )
                    }
                } else {
                    tree_end -= (tree_end - tree_start) / 2;
                    continue
                };

                debug!(
                    "\t\tBuilding tree at [{tree_start:#018x}..{:#018x}], {} pages",
                    tree_start + layout.node_count() * PAGE_SIZE,
                    layout.node_count(),
                );

                if let Some(mut ghost_pages) = ghost_pages {
                    debug!("\t\t\tReserving {ghost_pages} ghost pages");

                    #[cfg(debug_assertions)]
                    let [mut prev, mut prev_len] = [0, 0];

                    while ghost_pages != 0 {
                        let order = ghost_pages.ilog2();
                        ghost_pages -= 1 << order;

                        #[cfg_attr(not(debug_assertions), expect(unused))]
                        let index = unsafe { (*tree).alloc(order).expect("Couldn't reserve ghost pages") };

                        #[cfg(debug_assertions)]
                        if index == prev + prev_len {
                            prev = index;
                            prev_len = 1 << order;
                        } else {
                            panic!("Ghost page reservation wasn't contiguous");
                        }
                    }
                }

                unsafe { phys_alloc.push_tree(PAddr::new(tree_start), tree) }
                tree_start += layout.node_count() * PAGE_SIZE;
                tree_end = usable_end & !(PHYS_ALLOC_ALIGNMENT.as_usize() - 1);
            }
        }

        phys_alloc.sort_tree();
        match phys_alloc.tree_count() {
            0 => panic!("Couldn't initialize physical page allocator, found no usable memory regions"),
            n => {
                info!("Initialized physical page allocator with {n} trees");
                unsafe { set_phys_alloc(phys_alloc) }
            }
        }
    }

    // Virtual-map the framebuffer
    {
        let v_map = get_virtual_map();
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
                    VFlags::GLOBAL | VFlags::WRITABLE | VFlags::WRITE_THROUGH | VFlags::EXECUTE_DISABLE,
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

    // Setup device tree, which includes waking up all AP and setting up interrupt handlers
    // This calls the closure once for every CPU cores locally
    unsafe { init_device_tree(info.device_tree, scratch_pages, main) }
}

/// # Safety
/// - All [`statics`](elpytios_kernel::statics) must have been initialized prior to calling this
///   function.
/// - This function must be able to be run in parallel with itself on other threads
fn main(core_count: u32) -> ! {
    if CpuContext::get().is_bootstrap {
        info!("Hello, world! Kernel is now up and running on {core_count} logical processors!");
    }

    {
        use elpytios_bootinfo::PixelFormat;
        use elpytios_kernel::statics::get_frame_buffer;

        let fbo = get_frame_buffer();
        let fbo_div = fbo.height.div_ceil(core_count as usize);
        let cpu_id = CpuContext::get().cpu_id as usize;

        match fbo.format {
            fmt @ (PixelFormat::RGB_8_BIT | PixelFormat::BGR_8_BIT) => {
                let invert_br = matches!(fmt, PixelFormat::BGR_8_BIT);
                for y in (cpu_id * fbo_div)..((cpu_id + 1) * fbo_div).min(fbo.height) {
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
