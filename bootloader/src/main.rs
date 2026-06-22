#![feature(const_clone, const_iter, const_trait_impl, custom_inner_attributes)]
#![rustfmt::skip]

#![no_std]
#![no_main]

use core::{fmt::Write, mem::{MaybeUninit, transmute}};

use const_panic::concat_panic;
use elpytios_bootloader::{page_alloc::PhysicalPageAllocator, page_alloc_tree::PAGE_SIZE, rendering::{DisplayWriter, GraphicsInfo}};
use elpytios_elf::{Elf, Elf64, ElfSegment, ElfSegmentType};
use uefi::{Status, boot, entry, helpers, mem::memory_map::{MemoryMap, MemoryMapOwned}, proto::console::gop::*};

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

fn setup_uefi_and_exit() -> (GraphicsInfo, MemoryMapOwned) {
    let mut frame_buffer;
    let frame_buffer_size;
    let (w, h);
    let stride;
    let pixel_format;
    let frame_buffer_ptr;

    {
        helpers::init().unwrap();

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

        frame_buffer       = graphics_output_protocol.frame_buffer();
        frame_buffer_size  = frame_buffer.size();
        (w, h)             = mode_info.resolution();
        stride             = mode_info.stride();
        pixel_format       = mode_info.pixel_format();
        frame_buffer_ptr   = frame_buffer.as_mut_ptr();
    }

    let memory_map;
    unsafe {
        memory_map = boot::exit_boot_services(Some(boot::MemoryType::LOADER_DATA));
    }

    (GraphicsInfo {
        w: w,
        h: h,
        stride: stride,
        pixel_format: pixel_format,
        frame_buffer: frame_buffer_ptr,
        frame_buffer_size: frame_buffer_size
    }, memory_map)
}

#[entry]
fn entry() -> Status {
    let (graphics_info, memory_map) = setup_uefi_and_exit();

    let mut display_writer = DisplayWriter {
        graphics_info: &graphics_info,
        line: 0,
        col: 0
    };

    let a = display_writer.columns();
    let b = display_writer.lines();

    writeln!(&mut display_writer, "{}x{}", a, b).unwrap();

    let allocator = PhysicalPageAllocator::new(&memory_map).unwrap();
    unsafe {
        writeln!(&mut display_writer, "{:?}", allocator.alloc(4).unwrap()).unwrap();
        writeln!(&mut display_writer, "{:?}", allocator.alloc(1).unwrap()).unwrap();
        writeln!(&mut display_writer, "{:?}", allocator.alloc(1).unwrap()).unwrap();
        writeln!(&mut display_writer, "{:?}", allocator.alloc(1).unwrap()).unwrap();
        writeln!(&mut display_writer, "{:?}", allocator.alloc(1).unwrap()).unwrap();
        writeln!(&mut display_writer, "{:?}", allocator.alloc(120000).unwrap()).unwrap();
        writeln!(&mut display_writer, "{:?}", allocator.alloc(60000).unwrap()).unwrap();
        writeln!(&mut display_writer, "{:?}", allocator.alloc(60000).unwrap()).unwrap();
        writeln!(&mut display_writer, "{:?}", allocator.alloc(120000).unwrap()).unwrap();
        writeln!(&mut display_writer, "{}", allocator).unwrap();
    }

    let mut virt_base = usize::MAX;
    let mut virt_max = usize::MIN;

    for segment in KERNEL_SEGMENTS {
        if segment.segment_type != ElfSegmentType::Load { continue }
        virt_base = virt_base.min(segment.virtual_address);
        virt_max = virt_max.max(virt_base + segment.memory_size);
    }

    let kernel_memory = unsafe { allocator.alloc((virt_max - virt_base).div_ceil(PAGE_SIZE)) }.unwrap();
    for segment in KERNEL_SEGMENTS {
        if segment.segment_type != ElfSegmentType::Load { continue }
        unsafe {
            kernel_memory.add(segment.virtual_address - virt_base).copy_from_nonoverlapping(segment.data.as_ptr(), segment.data.len());
            kernel_memory.add(segment.data.len()).write_bytes(0, segment.memory_size - segment.data.len());
        }
    }

    unsafe {
        type EntryPoint = unsafe extern "sysv64" fn() -> !;

        let entry_point = kernel_memory.add(KERNEL_BINARY.program_entry() as usize);
        let entry_point = transmute::<*mut u8, EntryPoint>(entry_point);
        entry_point()
    }
}
