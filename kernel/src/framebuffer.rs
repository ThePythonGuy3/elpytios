use elpytios_bootinfo::PixelFormat;

#[derive(Debug)]
pub struct FrameBuffer {
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub format: PixelFormat,
    pub pointer: *mut u8,
}
