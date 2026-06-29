#![feature(const_cmp, const_convert, const_iter, const_trait_impl, custom_inner_attributes)]
#![rustfmt::skip]

#![no_std]
#![no_main]

use core::{arch::asm, mem::MaybeUninit};

use const_panic::concat_panic;
use elpytios_elf::{Elf, Elf64, ElfSegment64, ElfSegmentType, sys::{ElfRela64, ElfRela64Type}};
use elpytios_bootinfo::{BootInfo, GraphicsInfo, MAX_MEMORY_REGIONS, MemoryReclaimType, MemoryRegion, PAGE_SIZE, paddr::PAddr};
use uefi::{Status, boot::{self, AllocateType, MemoryType}, entry, helpers, mem::memory_map::{MemoryMap}, proto::console::gop::*};

const _: () = assert!(PAGE_SIZE == boot::PAGE_SIZE);

const MEM_KERNEL_CODE:    MemoryType = MemoryType::custom(0x8000_0000);
const MEM_STACK:          MemoryType = MemoryType::custom(0x8000_0001);
const MEM_PAGE_TABLE:     MemoryType = MemoryType::custom(0x8000_0002);
const MEM_BOOT_INFO:      MemoryType = MemoryType::custom(0x8000_0003);

const MEM_STACK_LEN:      usize      = 8;
const MEM_PAGE_TABLE_LEN: usize      = 1;
const MEM_BOOT_INFO_LEN:  usize      = size_of::<BootInfo>().div_ceil(PAGE_SIZE);

const KERNEL_BINARY: Elf64 = match Elf::from_bytes(include_bytes!(concat!("../../target/x86_64-unknown-none/", cfg_select! {
    debug_assertions => "bootloader_debug",
    _ => "bootloader",
}, "/elpytios-kernel"))) {
    Ok(Elf::N32(..)) => panic!("Expected 64-bit kernel ELF"),
    Ok(Elf::N64(elf)) => elf,
    Err(e) => concat_panic!(e),
};

const KERNEL_SEGMENTS: [ElfSegment64; KERNEL_BINARY.program_header_count()] = {
    let mut out: MaybeUninit<[ElfSegment64; _]> = MaybeUninit::uninit();
    let mut ptr = out.as_mut_ptr() as *mut ElfSegment64;

    for segment in KERNEL_BINARY.program_segments() {
        let segment = match segment {
            Ok(segment) => segment,
            Err(e) => concat_panic!(e),
        };

        if let ElfSegmentType::Load(..) = segment.segment_type && segment.alignment != PAGE_SIZE as u64 {
            concat_panic!("Kernel segments must be aligned to ", PAGE_SIZE, "! Found: ", segment.alignment);
        }

        unsafe {
            ptr.write(segment);
            ptr = ptr.add(1);
        }
    }

    unsafe { out.assume_init() }
};

/// Index 0: Lowest virtual address of the kernel.
/// Index 1: Highest virtual address of the kernel, page-aligned.
const KERNEL_VIRTUAL_ADDRESSES: [usize; 2] = {
    let mut min = u64::MAX;
    let mut max = u64::MIN;

    let mut i = 0;
    loop {
        if i == KERNEL_SEGMENTS.len() { break }

        let segment = &KERNEL_SEGMENTS[i];
        if !matches!(segment.segment_type, ElfSegmentType::Load(..)) {
            i += 1;
            continue
        }

        min = min.min(segment.virtual_address);
        max = max.max(segment.virtual_address + segment.memory_size);

        i += 1;
    }

    match (usize::try_from(min), usize::try_from(max)) {
        (Ok(min), Ok(max)) => [
            if min % PAGE_SIZE == 0 {
                min
            } else {
                concat_panic!("Virtual address base (", min, ") isn't aligned to ", PAGE_SIZE)
            },
            max.next_multiple_of(PAGE_SIZE)
        ],
        _ => concat_panic!("Integer doesn't fit: ", max),
    }
};

struct UefiInfo {
    pub kernel_entry:      *mut u8,
    pub kernel_stack_base: *mut u8,
    pub boot_info:     *mut BootInfo,
}

fn setup_uefi_and_exit() -> UefiInfo {
    let kernel_entry:      *mut u8;
    let kernel_stack_base: *mut u8;
    let boot_info:     *mut BootInfo;
    
    helpers::init().unwrap();

    {
        // Graphics Info Fetching
        let graphics_output_protocol_handle = boot::get_handle_for_protocol::<GraphicsOutput>().expect("No Graphics Output Protocol");
        let mut graphics_output_protocol;
        unsafe {
            graphics_output_protocol = boot::open_protocol::<GraphicsOutput>(
                boot::OpenProtocolParams {
                    handle: graphics_output_protocol_handle,
                    agent: boot::image_handle(),
                    controller: None
                },
                boot::OpenProtocolAttributes::GetProtocol
            ).expect("Error opening Graphics Output Protocol");
        }

        let mut max_area: usize            = 0;
        let mut max_mode: Option<Mode>     = None;
        let mut info:     Option<ModeInfo> = None;
        for possible_mode in graphics_output_protocol.modes() {
            let _info = possible_mode.info();

            let (w, h) = _info.resolution();
            let area = w * h;

            if max_mode.is_none() || area > max_area {
                max_area = area;
                max_mode = Some(possible_mode);
                info = Some(*_info);
            }
        }

        let mode:      &Mode     = &max_mode.unwrap();
        let mode_info: &ModeInfo = &info.unwrap();

        graphics_output_protocol.set_mode(mode).unwrap();

        let mut frame_buffer   = graphics_output_protocol.frame_buffer();
        let frame_buffer_size  = frame_buffer.size();
        let (w, h)             = mode_info.resolution();
        let stride             = mode_info.stride();
        let pixel_format       = mode_info.pixel_format();
        let frame_buffer_ptr   = frame_buffer.as_mut_ptr();

        let _graphics_info = GraphicsInfo {
            w,
            h,
            stride,
            pixel_format: match pixel_format {
                PixelFormat::Rgb     => elpytios_bootinfo::PixelFormat::RGB_8_BIT,
                PixelFormat::Bgr     => elpytios_bootinfo::PixelFormat::BGR_8_BIT,
                PixelFormat::Bitmask => elpytios_bootinfo::PixelFormat::BIT_MASK,
                PixelFormat::BltOnly => elpytios_bootinfo::PixelFormat::BLT_ONLY
            },
            frame_buffer: frame_buffer_ptr,
            frame_buffer_size: frame_buffer_size,
        };

        /*let mut virtual_map = unsafe {
            VirtualMapBuilder::new(
                // 511 used for higher-half addressing
                510,
                // Allocate 1 page via UEFI's allocator
                || boot::allocate_pages(AllocateType::AnyPages, ELPYTI_PAGE_TABLE, 1).ok().map(|ptr| {
                    let ptr = ptr.as_ptr();
                    ptr.write_bytes(0, PAGE_SIZE);
                    PAddr::new(ptr.addr())
                }),
                // Identity mapping is still enabled at this point
                |ptr| ptr.addr() as *mut (),
            )
        };*/

        let [virtual_base, virtual_max] = KERNEL_VIRTUAL_ADDRESSES;
        let kernel_base_pages = (virtual_max - virtual_base) / PAGE_SIZE;
        let kernel_ptr = boot::allocate_pages(
            AllocateType::AnyPages,
            MEM_KERNEL_CODE,
            kernel_base_pages,
        ).unwrap().as_ptr();

        for segment in KERNEL_SEGMENTS {
            let ElfSegmentType::Load(data) = segment.segment_type else { continue };
            unsafe {
                kernel_ptr
                    .add(segment.virtual_address as usize - virtual_base)
                    .copy_from_nonoverlapping(data.as_ptr(), data.len());

                kernel_ptr
                    .add(segment.virtual_address as usize - virtual_base + data.len())
                    .write_bytes(0, segment.memory_size as usize - data.len());
            }

            /*for i in (0..segment.memory_size as usize).step_by(PAGE_SIZE) {
                virtual_map.map(
                    PAddr::new(unsafe { kernel_ptr.add(segment.virtual_address as usize - virtual_base).addr() } + i),
                    VAddr::new(kernel_ptr.addr() + segment.virtual_address as usize - virtual_base + i),
                    VFlags::GLOBAL | match segment.flags.contains(ElfProgramFlags::WRITABLE) {
                        false => VFlags::empty(),
                        true => VFlags::WRITABLE,
                    },
                ).unwrap_or_else(|e| panic!("Couldn't identity-map: {e}"));

                virtual_map.map(
                    PAddr::new(unsafe { kernel_ptr.add(segment.virtual_address as usize - virtual_base).addr() } + i),
                    VAddr::new(segment.virtual_address as usize - virtual_base + HIGHER_HALF_ADDRESS + i),
                    VFlags::GLOBAL | match segment.flags.contains(ElfProgramFlags::WRITABLE) {
                        false => VFlags::empty(),
                        true => VFlags::WRITABLE,
                    },
                ).unwrap_or_else(|e| panic!("Couldn't higher-half map: {e}"));
            }*/
        }

        /*for i in 0..KERNEL_STACK_PAGES {
            virtual_map.map(
                PAddr::new(unsafe { kernel_ptr.add((kernel_base_pages + i) * PAGE_SIZE).addr() }),
                VAddr::new(kernel_ptr.addr() + (kernel_base_pages + i) * PAGE_SIZE),
                VFlags::GLOBAL | VFlags::WRITABLE,
            ).unwrap_or_else(|e| panic!("Couldn't identity-map: {e}"));

            virtual_map.map(
                PAddr::new(unsafe { kernel_ptr.add((kernel_base_pages + i) * PAGE_SIZE).addr() }),
                VAddr::new(virtual_max - virtual_base + HIGHER_HALF_ADDRESS + i * PAGE_SIZE),
                VFlags::GLOBAL | VFlags::WRITABLE,
            ).unwrap_or_else(|e| panic!("Couldn't higher-half map: {e}"));
        }*/

        for segment in KERNEL_SEGMENTS {
            let ElfSegmentType::Dynamic { offset, size, stride } = segment.segment_type else { continue };
            for i in 0..size / stride {
                unsafe {
                    let rela = kernel_ptr.cast::<ElfRela64>().byte_add(offset - virtual_base).add(i).read_unaligned();
                    match rela.info.kind {
                        ElfRela64Type::X86_64_RELATIVE => {
                            let slide = kernel_ptr.addr() as i64 - virtual_base as i64;
                            let patch_addr = kernel_ptr.add(rela.offset as usize - virtual_base);
                            let value = slide + rela.addend;
                            patch_addr.cast::<i64>().write(value);
                        }
                        kind => panic!("Unsupported Elf64_Rela kind: {}", kind.0),
                    }
                }
            }
        }

        let stack_ptr = boot::allocate_pages(
            AllocateType::AnyPages,
            MEM_STACK,
            MEM_STACK_LEN,
        ).unwrap().as_ptr();

        kernel_entry = unsafe { kernel_ptr.add(KERNEL_BINARY.program_entry() as usize - virtual_base) };
        kernel_stack_base = unsafe { stack_ptr.add((MEM_STACK_LEN) * PAGE_SIZE) };

        //let (root_page_table_ret, virtual_map) = virtual_map.finish().unwrap();
        //root_page_table = root_page_table_ret;
        //kernel_offset = HIGHER_HALF_ADDRESS - kernel_ptr.addr();

        unsafe {
            let page_table_init = boot::allocate_pages(AllocateType::AnyPages, MEM_PAGE_TABLE, MEM_PAGE_TABLE_LEN).unwrap().as_ptr();
            page_table_init.write_bytes(0, MEM_PAGE_TABLE_LEN * PAGE_SIZE);

            boot_info = boot::allocate_pages(
                AllocateType::AnyPages,
                MEM_BOOT_INFO,
                MEM_BOOT_INFO_LEN,
            ).unwrap().as_ptr().cast::<BootInfo>();
            boot_info.write(BootInfo {
                kernel_base: PAddr::new(kernel_ptr.addr()),
                kernel_pages: kernel_base_pages + MEM_STACK_LEN,
                page_table_init: PAddr::new(page_table_init.addr()),
                page_table_init_len: MEM_PAGE_TABLE_LEN,

                memory_regions_base: [MaybeUninit::uninit(); _],
                memory_regions_size: 0,
            });
        }

        /*
        kernel_entry = VAddr::new(KERNEL_BINARY.program_entry() as usize);

        // Identity-map the kernel switcher
        let switcher_addr = (switch_to_kernel as *const ()).addr();
        assert_eq!(switcher_addr % PAGE_SIZE, 0, "`switch_to_kernel` must be page-aligned");
        virtual_map.map(PAddr::new(switcher_addr), VAddr::new(switcher_addr), VFlags::empty()).unwrap_or_else(|e| panic!("{e}"));

        let mut next_free_page = |p_addr: PAddr, page_count: usize, flags: VFlags| {
            let v_addr = next_v_addr;
            next_v_addr += page_count * PAGE_SIZE;

            for i in 0..page_count {
                let offset = i * PAGE_SIZE;
                virtual_map.map(
                    PAddr::new(p_addr.addr() + offset),
                    VAddr::new(v_addr + offset),
                    VFlags::GLOBAL | flags,
                ).unwrap_or_else(|e| panic!("{e}"));
            }

            VAddr::new(v_addr)
        };

        // Map the stack pointer
        let stack_ptr = boot::allocate_pages(AllocateType::AnyPages, ELPYTI_KERNEL_STACK, KERNEL_STACK_PAGES).unwrap().as_ptr();
        let stack = next_free_page(PAddr::new(stack_ptr.addr()), KERNEL_STACK_PAGES, VFlags::WRITABLE);
        kernel_stack_base = VAddr::new(stack.addr() + KERNEL_STACK_PAGES * PAGE_SIZE);

        // Map the framebuffer
        let fb_phys = frame_buffer_ptr.addr();
        let fb_size = frame_buffer_size;

        let fb_phys_base = fb_phys & !(PAGE_SIZE - 1);
        let fb_phys_end = (fb_phys + fb_size).next_multiple_of(PAGE_SIZE);
        let fb_page_count = (fb_phys_end - fb_phys_base) / PAGE_SIZE;

        let (pml4_phys_ret, virtual_map) = virtual_map.finish().unwrap();
        pml4_phys = pml4_phys_ret;
        unsafe {
            boot_info_ptr = kernel_ptr.add(KERNEL_BOOTINFO_ADDRESS - virtual_base).cast();
            boot_info_ptr.write(BootInfo {
                graphics_info,
                virtual_map,
                switcher_map: VAddr::new(switcher_addr),

                // Initialized after exiting UEFI boot services
                memory_regions_base: [MaybeUninit::uninit(); _],
                memory_regions_size: 0,

                v_addr_start: VAddr::new(next_v_addr),
                v_addr_end: VAddr::new(usize::MAX),
            });
        }*/
    }

    unsafe {
        let memory_map = boot::exit_boot_services(Some(MemoryType::LOADER_DATA));

        let mut len = 0;
        //let mut region = None;

        for mut entry in memory_map.entries().copied() {
            let reclaim = match entry.ty {
                MemoryType::CONVENTIONAL => MemoryReclaimType::Free,
                MemoryType::LOADER_CODE | MemoryType::LOADER_DATA |
                MemoryType::BOOT_SERVICES_CODE | MemoryType::BOOT_SERVICES_DATA => MemoryReclaimType::AfterVirtualMapping,
                _ => continue,
            };

            // Skip the page if it contains null pointer
            if entry.phys_start == 0 {
                entry.phys_start += PAGE_SIZE as u64;
                entry.page_count -= 1;
            }

            (&raw mut (*boot_info).memory_regions_base[len])
                .cast::<MemoryRegion>()
                .write(MemoryRegion {
                    base: PAddr::new(entry.phys_start as usize),
                    pages: entry.page_count as usize,
                    reclaim,
                });
            len += 1;

            if len == MAX_MEMORY_REGIONS {
                break
            }

            /*let start = entry.phys_start as usize;
            let count = entry.page_count as usize;

            match region.as_mut() {
                None => region = Some(MemoryRegion {
                    base: PAddr::new(start),
                    pages: count,
                }),
                Some(reg) => {
                    if reg.base.addr() + reg.pages * PAGE_SIZE == start {
                        reg.pages += count;
                    } else {
                        (&raw mut (*boot_info_ptr).memory_regions_base[len])
                            .cast::<MemoryRegion>()
                            .write(mem::replace(reg, MemoryRegion {
                                base: PAddr::new(start),
                                pages: count,
                            }));

                        len += 1;
                        if len == MAX_MEMORY_REGIONS {
                            break
                        }
                    }
                }
            }*/
        }

        /*if len < MAX_MEMORY_REGIONS && let Some(region) = region {
            (&raw mut (*boot_info_ptr).memory_regions_base[len]).cast::<MemoryRegion>().write(region);
            len += 1;
        }*/

        //slice::from_raw_parts_mut(&raw mut (*boot_info_ptr).memory_regions_base as *mut MemoryRegion, len).sort_unstable_by(|a, b| b.pages.cmp(&a.pages));

        (&raw mut (*boot_info).memory_regions_size).write(len);
    }

    UefiInfo {
        kernel_stack_base,
        kernel_entry,
        boot_info,
    }
}

#[entry]
fn entry() -> Status {
    let UefiInfo { kernel_entry, kernel_stack_base, boot_info } = setup_uefi_and_exit();
    unsafe {
        asm!(
            "mov rdi, {boot_info}",
            "lea rsp, [{kernel_stack_base} - 8]",
            "jmp {kernel_entry}",

            boot_info = in(reg) boot_info,
            kernel_stack_base = in(reg) kernel_stack_base,
            kernel_entry = in(reg) kernel_entry,

            options(noreturn)
        )
        /*asm!(
            "lea rsp, [{kernel_stack_base} - 8]",
            "mov rdi, {root_page_table}",
            "mov rsi, {kernel_offset}",
            "jmp {kernel_entry}",

            kernel_stack_base = in(reg) kernel_stack_base,
            kernel_entry = in(reg) kernel_entry,
            root_page_table = in(reg) root_page_table.addr(),
            kernel_offset = in(reg) kernel_offset,

            optons(noreturn)
        )*/
    }
}
