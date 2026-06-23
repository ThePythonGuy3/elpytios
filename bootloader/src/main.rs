#![feature(const_clone, const_iter, const_trait_impl, custom_inner_attributes)]
#![rustfmt::skip]

#![no_std]
#![no_main]

use core::{mem::MaybeUninit, arch::asm};

use const_panic::concat_panic;
use elpytios_elf::{Elf, Elf64, ElfSegment64, ElfSegmentType};
use elpytios_bootinfo::{BootInfo, GraphicsInfo};
use uefi::{Status, boot::{self, AllocateType, MemoryType}, entry, helpers, mem::memory_map::MemoryMapOwned, proto::console::gop::*};

const PAGE_SIZE: usize = 4096;

static KERNEL_BINARY: Elf64 = match Elf::from_bytes(include_bytes!("../../target/x86_64-unknown-none/bootloader/elpytios-kernel")) {
    Ok(Elf::N32) => panic!("Expected 64-bit kernel ELF"),
    Ok(Elf::N64(elf)) => elf,
    Err(e) => concat_panic!(e),
};

const KERNEL_SEGMENTS: [ElfSegment64; KERNEL_BINARY.program_header_count()] = {
    let mut out: MaybeUninit<[ElfSegment64; _]> = MaybeUninit::uninit();
    let mut ptr = out.as_mut_ptr() as *mut ElfSegment64;

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

// TODO page allocator
struct UefiInfo {
    pub memory_map:         MemoryMapOwned,

    pub kernel_stack_base: *mut u8,
    pub kernel_entry:      *mut u8,

    pub boot_info_mapping: *mut BootInfo,
}

fn setup_uefi_and_exit() -> UefiInfo {
    let graphics_info:      GraphicsInfo;
    let memory_map:         MemoryMapOwned;

    let kernel_stack_base: *mut u8;
    let kernel_entry:      *mut u8;

    let boot_info_mapping: *mut BootInfo;

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

        let kernel_page_table = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1).unwrap().as_ptr().cast::<u64>();
        unsafe {
            kernel_page_table.write_bytes(0, PAGE_SIZE / size_of::<u64>());
        }

        for segment in KERNEL_SEGMENTS {
            if segment.segment_type != ElfSegmentType::Load { continue }

            let segment_size = (segment.memory_size as usize).next_multiple_of(PAGE_SIZE);
            let segment_ptr = boot::allocate_pages(
                AllocateType::Address(segment.virtual_address),
                MemoryType::LOADER_CODE,
                segment_size / PAGE_SIZE,
            ).unwrap_or_else(|_| panic!("Couldn't allocate kernel section data to {:x}", segment.virtual_address)).as_ptr();

            unsafe {
                segment_ptr.copy_from_nonoverlapping(segment.data.as_ptr(), segment.data.len());
                segment_ptr.add(segment.data.len()).write_bytes(0, segment_size - segment.data.len());
            }
        }

        let stack = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, KERNEL_STACK_PAGES).unwrap().as_ptr();
        kernel_stack_base = unsafe { stack.add(KERNEL_STACK_PAGES * PAGE_SIZE) };
        kernel_entry = KERNEL_BINARY.program_entry() as *mut u8;

        let boot_info = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, size_of::<BootInfo>().div_ceil(PAGE_SIZE)).unwrap().as_ptr();
        let boot_info = unsafe { boot_info.add(boot_info.align_offset(align_of::<BootInfo>())) }.cast::<BootInfo>();
        unsafe {
            boot_info.write_volatile(BootInfo {
                graphics_info
            });
        }

        boot_info_mapping = boot_info;
    }

    unsafe {
        memory_map = boot::exit_boot_services(Some(boot::MemoryType::LOADER_DATA));
    }

    UefiInfo {
        memory_map,

        kernel_stack_base,
        kernel_entry,

        boot_info_mapping,
    }
}

#[entry]
fn entry() -> Status {
    let uefi_info = setup_uefi_and_exit();
    unsafe {
        asm!(
            "mov rdi, {boot_info_base}",
            "mov rsp, {stack_base}",
            "jmp {entry_point}",

            boot_info_base = in(reg) uefi_info.boot_info_mapping as usize,
            stack_base = in(reg) uefi_info.kernel_stack_base,
            entry_point = in(reg) uefi_info.kernel_entry,

            options(noreturn)
        )
    }
}