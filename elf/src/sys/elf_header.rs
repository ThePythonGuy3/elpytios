use bytemuck::AnyBitPattern;

/// The ELF header is always found at the start of the file.
#[derive(Debug, Clone, Copy, AnyBitPattern)]
#[repr(C)]
pub struct ElfHeaderPrologue {
    /// Magic number - 0x7F, then 'ELF' in ASCII                      0-3
    pub magic: [u8; 4],
    /// 1 = 32 bit, 2 = 64 bit                                        4
    pub arch: u8,
    /// 1 = little endian, 2 = big endian                             5
    pub endian: u8,
    /// ELF header version                                            6
    pub header_version: u8,
    /// OS ABI - usually 0 for System V                               7
    pub os_abi: u8,
    /// Unused/padding                                                8-15
    _padding: [u8; 8],
    /// Type (1 = relocatable, 2 = executable, 3 = shared, 4 = core) 16-17
    pub elf_type: ElfType,
    /// Instruction set - see table below                            18-19
    pub instruction_set: u16,
    /// ELF Version (currently 1)                                    20-23
    pub elf_version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, AnyBitPattern)]
#[repr(transparent)]
pub struct ElfType(u16);
impl ElfType {
    pub const NONE: Self = Self(0);
    pub const RELOCATABLE: Self = Self(1);
    pub const EXECUTABLE: Self = Self(2);
    pub const DYNAMIC: Self = Self(3);
    pub const CORE_DUMP: Self = Self(4);
}

/// Continuation of [`ElfHeaderPrologue`] in 64-bit format (arch == 2).
#[derive(Debug, Clone, Copy, AnyBitPattern)]
#[repr(C)]
pub struct ElfHeader64 {
    /// Program entry offset                                         24-31
    pub program_entry_offset: u64,
    /// Program header table offset                                  32-39
    pub program_header_table_offset: u64,
    /// Section header table offset                                  40-47
    pub section_header_table_offset: u64,
    /// Flags - architecture dependent; see note below               48-51
    pub flags: u32,
    /// ELF Header size                                              52-53
    pub header_size: u16,
    /// Size of an entry in the program header table                 54-55
    pub program_header_entry_size: u16,
    /// Number of entries in the program header table                56-57
    pub program_header_entry_len: u16,
    /// Size of an entry in the section header table                 58-59
    pub section_header_entry_size: u16,
    /// Number of entries in the section header table                60-61
    pub section_header_entry_len: u16,
    /// Section index to the section header string table             62-63
    pub section_header_string_table_index: u16,
}
