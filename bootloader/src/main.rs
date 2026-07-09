#![feature(const_cmp, const_convert, const_iter, const_trait_impl, custom_inner_attributes)]
#![rustfmt::skip]

#![no_std]
#![no_main]

use core::{arch::asm, mem::{self, MaybeUninit}};

use arrayvec::ArrayVec;
use const_panic::concat_panic;
use elpytios_elf::{Elf, Elf64, ElfSegment64, ElfSegmentType, sys::{ElfProgramFlags, ElfRela64, ElfRela64Type}};
use elpytios_bootinfo::{BootInfo, DeviceTree, GraphicsInfo, IdentityMap, IdentityMapFlags, MAX_SCRATCH, MemoryRegion, PAGE_SIZE, Reloc, paddr::PAddr};
use uefi::{Status, boot::{self, AllocateType, MemoryType}, entry, helpers, mem::memory_map::MemoryMap, proto::console::gop::*, table::cfg::ConfigTableEntry};

const _: () = assert!(PAGE_SIZE == boot::PAGE_SIZE);

const MEM_KERNEL_CODE:  MemoryType = MemoryType::custom(0x8000_0000);
const MEM_STACK:        MemoryType = MemoryType::custom(0x8000_0001);
const MEM_BOOT_INFO:    MemoryType = MemoryType::custom(0x8000_0002);
const MEM_SCRATCH:      MemoryType = MemoryType::custom(0x8000_0003);

const MEM_STACK_LEN:     usize = 16; // Note: `opt-level = 0` makes the code consume way too much stack space
const MEM_BOOT_INFO_LEN: usize = size_of::<BootInfo>().div_ceil(PAGE_SIZE);
const MEM_SCRATCH_LEN:   usize = MAX_SCRATCH;

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
            if min.is_multiple_of(PAGE_SIZE) {
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
    pub boot_info:         *mut BootInfo,
}

fn setup_uefi_and_exit() -> UefiInfo {
    let kernel_entry:      *mut u8;
    let kernel_stack_base: *mut u8;
    let boot_info:         *mut BootInfo;
    
    helpers::init().unwrap();

    {
        // Graphics Info Fetching
        let mut graphics_output_protocol = boot::open_protocol_exclusive::<GraphicsOutput>(
            boot::get_handle_for_protocol::<GraphicsOutput>().expect("No Graphics Output Protocol")
        ).expect("Error opening Graphics Output Protocol");

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

        let graphics_info = GraphicsInfo {
            w,
            h,
            stride,
            pixel_format: match pixel_format {
                PixelFormat::Rgb     => elpytios_bootinfo::PixelFormat::RGB_8_BIT,
                PixelFormat::Bgr     => elpytios_bootinfo::PixelFormat::BGR_8_BIT,
                PixelFormat::Bitmask => elpytios_bootinfo::PixelFormat::BIT_MASK,
                PixelFormat::BltOnly => elpytios_bootinfo::PixelFormat::BLT_ONLY
            },
            frame_buffer: PAddr::new(frame_buffer_ptr.addr()),
            frame_buffer_size: frame_buffer_size,
        };

        let [virtual_base, virtual_max] = KERNEL_VIRTUAL_ADDRESSES;
        let mut identity_maps = ArrayVec::new();

        let kernel_base_pages = (virtual_max - virtual_base) / PAGE_SIZE;
        let kernel_ptr = boot::allocate_pages(AllocateType::AnyPages, MEM_KERNEL_CODE, kernel_base_pages).unwrap().as_ptr();

        for segment in KERNEL_SEGMENTS {
            let ElfSegmentType::Load(data) = segment.segment_type else { continue };
            identity_maps.push(IdentityMap::new(
                PAddr::new(kernel_ptr.addr() + segment.virtual_address as usize - virtual_base),
                (segment.memory_size as usize).div_ceil(PAGE_SIZE),
                {
                    let mut flags = IdentityMapFlags::empty();
                    if segment.flags.contains(ElfProgramFlags::EXECUTABLE) { flags |= IdentityMapFlags::EXECUTABLE }
                    if segment.flags.contains(ElfProgramFlags::READABLE) { flags |= IdentityMapFlags::READABLE }
                    if segment.flags.contains(ElfProgramFlags::WRITABLE) { flags |= IdentityMapFlags::WRITABLE }

                    flags
                },
            ));

            unsafe {
                kernel_ptr
                    .add(segment.virtual_address as usize - virtual_base)
                    .copy_from_nonoverlapping(data.as_ptr(), data.len());

                kernel_ptr
                    .add(segment.virtual_address as usize - virtual_base + data.len())
                    .write_bytes(0, segment.memory_size as usize - data.len());
            }
        }

        let mut relocations = ArrayVec::new();
        for segment in KERNEL_SEGMENTS {
            let ElfSegmentType::Dynamic { offset, size, stride } = segment.segment_type else { continue };
            relocations.push(Reloc { offset, size, stride });
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

        let stack_ptr = boot::allocate_pages(AllocateType::AnyPages, MEM_STACK, MEM_STACK_LEN).unwrap().as_ptr();
        identity_maps.push(IdentityMap::new(PAddr::new(stack_ptr.addr()), MEM_STACK_LEN, IdentityMapFlags::READABLE | IdentityMapFlags::WRITABLE));

        kernel_entry = unsafe { kernel_ptr.add(KERNEL_BINARY.program_entry() as usize - virtual_base) };
        kernel_stack_base = unsafe { stack_ptr.add(MEM_STACK_LEN * PAGE_SIZE) };

        boot_info = boot::allocate_pages(AllocateType::AnyPages, MEM_BOOT_INFO, MEM_BOOT_INFO_LEN).unwrap().as_ptr().cast();
        identity_maps.push(IdentityMap::new(PAddr::new(boot_info.addr()), MEM_BOOT_INFO_LEN, IdentityMapFlags::READABLE));

        //
        unsafe {
            let scratch_ptr = boot::allocate_pages(AllocateType::MaxAddress(1 << 16), MEM_SCRATCH, MEM_SCRATCH_LEN).unwrap().as_ptr();
            scratch_ptr.write_bytes(0, MEM_SCRATCH_LEN * PAGE_SIZE);
            let mut scratch_pages = ArrayVec::new();
            for i in 0..MEM_SCRATCH_LEN {
                scratch_pages.push(PAddr::new(scratch_ptr.addr() + i * PAGE_SIZE));
            }

            let device_tree = uefi::system::with_config_table(|slice| {
                let mut out = None;
                for i in slice {
                    match i.guid {
                        ConfigTableEntry::ACPI_GUID if out.is_none() => out = Some(DeviceTree::Acpi(PAddr::new(i.address.addr()))),
                        ConfigTableEntry::ACPI2_GUID => {
                            out = Some(DeviceTree::Acpi2(PAddr::new(i.address.addr())));
                            break
                        },
                        _ => {}
                    }
                }

                out.expect("No ACPI or ACPI2 table found")
            });

            boot_info.write(BootInfo {
                graphics_info,
                device_tree,

                kernel_elf_base: PAddr::new(kernel_ptr.addr()),
                kernel_virt_base: virtual_base,

                memory_regions: ArrayVec::new(),
                identity_maps,
                scratch_pages,
                relocations,
            });
        }
    }

    unsafe {
        let memory_map = boot::exit_boot_services(Some(MemoryType::LOADER_DATA));
        for entry in memory_map.entries() {
            if matches!(entry.ty, MemoryType::ACPI_RECLAIM) {
                (*boot_info).identity_maps.push(IdentityMap {
                    region: MemoryRegion::at(PAddr::new(entry.phys_start as usize), entry.page_count as usize),
                    flags: IdentityMapFlags::READABLE,
                });
            }
        }

        let mut region = None;
        for entry in memory_map.entries() {
            if !matches!(entry.ty,
                MemoryType::LOADER_CODE | MemoryType::LOADER_DATA |
                MemoryType::BOOT_SERVICES_CODE | MemoryType::BOOT_SERVICES_DATA |
                MemoryType::CONVENTIONAL
            ) {
                continue
            }
 
            let start = entry.phys_start as usize;
            let count = entry.page_count as usize;

            match region.as_mut() {
                None => region = Some(MemoryRegion::at(PAddr::new(start), count)),
                Some(reg) => {
                    if reg.base.addr() + reg.pages * PAGE_SIZE == start {
                        reg.pages += count;
                    } else {
                        if (*boot_info).memory_regions.try_push(mem::replace(reg, MemoryRegion::at(PAddr::new(start), count))).is_err() {
                            break
                        }
                    }
                }
            }
        }

        if let Some(region) = region {
            _ = (*boot_info).memory_regions.try_push(region);
        }

        (*boot_info).memory_regions.sort_unstable_by(|a, b| b.pages.cmp(&a.pages));
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
            "mov rsp, {kernel_stack_base}",
            "jmp {kernel_entry}",

            in("rdi") boot_info,
            kernel_stack_base = in(reg) kernel_stack_base,
            kernel_entry = in(reg) kernel_entry,

            options(noreturn)
        )
    }
}
