#![feature(const_clone, const_iter, const_trait_impl, custom_inner_attributes)]
#![rustfmt::skip]

#![no_std]
#![no_main]

use core::{mem::{MaybeUninit, transmute}, arch::asm};

use const_panic::concat_panic;
use elpytios_elf::{Elf, Elf64, ElfSegment, ElfSegmentType};
use elpytios_bootinfo::{BootInfo, GraphicsInfo};
use uefi::{Status, boot::{self, AllocateType, MemoryType}, entry, helpers, mem::memory_map::MemoryMapOwned, proto::console::gop::*};

const PAGE_SIZE: usize = 4096;

static KERNEL_BINARY: Elf64 = match Elf::from_bytes(include_bytes!(concat!("../../target/x86_64-unknown-none/", cfg_select! {
    debug_assertions => "debug",
    _ => "release",
}, "/elpytios-kernel"))) {
    Ok(Elf::N32) => panic!("Expected 64-bit kernel ELF"),
    Ok(Elf::N64(elf)) => elf,
    Err(e) => concat_panic!(e),
};

const KERNEL_SEGMENTS: [ElfSegment; KERNEL_BINARY.program_header_count()] = {
    let mut out: MaybeUninit<[ElfSegment; _]> = MaybeUninit::uninit();
    let mut ptr = out.as_mut_ptr() as *mut ElfSegment;

    for segment in KERNEL_BINARY.clone() {
        let segment = match segment {
            Ok(segment) => segment,
            Err(e) => concat_panic!(e),
        };

        unsafe {
            ptr.write(segment);
            ptr = ptr.add(1);
        }
    }

    unsafe { out.assume_init() }
};

const KERNEL_STACK_PAGES: usize = 4;

fn get_kernel_virtual_base_and_pages() -> Option<(usize, usize)> {
    let mut virt_base = usize::MAX;
    let mut virt_max  = usize::MIN;

    for segment in KERNEL_SEGMENTS {
        if segment.segment_type != ElfSegmentType::Load { continue }
        virt_base = virt_base.min(segment.virtual_address);
        virt_max = virt_max.max(virt_base + segment.memory_size);
    }

    if virt_base >= virt_max {
        return None;
    }

    Some((virt_base, (virt_max - virt_base).div_ceil(PAGE_SIZE)))
}

struct UefiInfo {
    pub graphics_info: GraphicsInfo,

    pub memory_map: MemoryMapOwned,

    pub kernel_mapping:     *mut u8,
    pub kernel_virtual_base: usize,
    pub kernel_pages:        usize,

    pub kernel_stack_mapping: *mut u8,
    pub kernel_stack_pages:    usize,

    pub boot_info_mapping: *mut BootInfo,
    pub boot_info_pages:    usize
}

fn setup_uefi_and_exit() -> UefiInfo {
    let graphics_info:   GraphicsInfo;

    let memory_map:      MemoryMapOwned;

    let kernel_mapping:     *mut u8;
    let kernel_virtual_base: usize;
    let kernel_pages:        usize;

    let kernel_stack_mapping: *mut u8;
    let kernel_stack_pages:    usize;

    let boot_info_mapping: *mut BootInfo;
    let boot_info_pages:    usize;

    {
        helpers::init().unwrap();

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

        graphics_info = GraphicsInfo {
            w: w,
            h: h,
            stride: stride,
            pixel_format: match pixel_format {
                PixelFormat::Rgb     => elpytios_bootinfo::PixelFormat::RGB_8_BIT,
                PixelFormat::Bgr     => elpytios_bootinfo::PixelFormat::BGR_8_BIT,
                PixelFormat::Bitmask => elpytios_bootinfo::PixelFormat::BIT_MASK,
                PixelFormat::BltOnly => elpytios_bootinfo::PixelFormat::BLT_ONLY
            },
            frame_buffer: frame_buffer_ptr,
            frame_buffer_size: frame_buffer_size
        };

        // Kernel Mapping
        (kernel_virtual_base, kernel_pages) = get_kernel_virtual_base_and_pages().unwrap();

        kernel_mapping = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, kernel_pages).unwrap().as_ptr();

        kernel_stack_pages   = KERNEL_STACK_PAGES;
        kernel_stack_mapping = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, kernel_stack_pages).unwrap().as_ptr();

        boot_info_pages   = size_of::<BootInfo>().div_ceil(PAGE_SIZE);
        boot_info_mapping = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, boot_info_pages).unwrap().as_ptr().cast::<BootInfo>();
    }

    unsafe {
        memory_map = boot::exit_boot_services(Some(boot::MemoryType::LOADER_DATA));
    }

    UefiInfo {
        graphics_info,

        memory_map,

        kernel_mapping,
        kernel_virtual_base,
        kernel_pages,

        kernel_stack_mapping,
        kernel_stack_pages,

        boot_info_mapping,
        boot_info_pages
    }
}

#[entry]
fn entry() -> Status {
    let uefi_info = setup_uefi_and_exit();

    for segment in KERNEL_SEGMENTS {
        if segment.segment_type != ElfSegmentType::Load { continue }
        unsafe {
            let segment_base = uefi_info.kernel_mapping.add(segment.virtual_address - uefi_info.kernel_virtual_base);

            segment_base
                .copy_from_nonoverlapping(segment.data.as_ptr(), segment.data.len());

            segment_base
                .add(segment.data.len())
                .write_bytes(0, segment.memory_size - segment.data.len());
        }
    }

    unsafe {
        type EntryPoint = unsafe extern "sysv64" fn(graphics_info: GraphicsInfo) -> !;

        let entry_point = uefi_info.kernel_mapping.add(KERNEL_BINARY.program_entry() as usize - uefi_info.kernel_virtual_base);
        let entry_point = transmute::<*mut u8, EntryPoint>(entry_point);

        let stack_base = uefi_info.kernel_stack_mapping as usize + uefi_info.kernel_stack_pages * PAGE_SIZE;

        uefi_info.boot_info_mapping.write_volatile(BootInfo {
            graphics_info: uefi_info.graphics_info
        });

        asm!(
            "mov rdi, {boot_info_base}",
            "mov rsp, {stack_base}",
            "jmp {entry_point}",
            boot_info_base = in(reg) uefi_info.boot_info_mapping as usize,
            stack_base = in(reg) stack_base,
            entry_point = in(reg) entry_point
        );

        loop {}
    }
}
