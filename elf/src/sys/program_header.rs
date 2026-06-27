use bitflags::bitflags;
use bytemuck::AnyBitPattern;

#[derive(Debug, Clone, Copy, AnyBitPattern)]
#[repr(C)]
pub struct ElfProgramHeader64 {
    /// Type of segment (see below)                                         0-3
    pub segment_type: u32,
    /// Flags (see below)                                                   4-7
    pub flags: ElfProgramFlags,
    /// The offset in the file that the data for this segment can be found  8-15
    pub segment_offset: u64,
    /// Where you should start to put this segment in virtual memory       16-23
    pub segment_virtual_address: u64,
    /// Reserved for segment's physical address                            24-31
    pub segment_physical_address: u64,
    /// Size of the segment in the file                                    32-39
    pub segment_file_size: u64,
    /// Size of the segment in memory                                      40-47
    pub segment_memory_size: u64,
    /// The required alignment for this section                            48-55
    pub section_alignment: u64,
}

#[derive(Debug, Clone, Copy, AnyBitPattern)]
#[repr(transparent)]
pub struct ElfProgramFlags(u32);
bitflags! {
    impl ElfProgramFlags: u32 {
        const EXECUTABLE = 1 << 0;
        const WRITABLE = 1 << 1;
        const READABLE = 1 << 2;
    }
}
