#![no_std]
#![no_main]

use core::{ptr::null_mut, time::Duration};

use uefi::{Status, boot, entry, helpers, mem::memory_map::MemoryMapOwned, println, proto::console::gop::*};

#[derive(Clone, Copy)]
struct GraphicsInfo {
    pub w:                 usize,
    pub h:                 usize,
    pub stride:            usize,
    pub pixel_format:      PixelFormat,
    pub frame_buffer:     *mut u8,
    pub frame_buffer_size: usize
}

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
    let (graphics_info, _memory_map) = setup_uefi_and_exit();

    let mut i = 0;
    for y in 0..graphics_info.h {
        for x in 0..graphics_info.w {
            unsafe {
                graphics_info.frame_buffer.cast::<u32>().add(x + y * graphics_info.stride).write_volatile(i);
            }

            i += 8;
        }
    }

    loop {}
}
