use bitflags::bitflags;
use bytemuck::AnyBitPattern;

#[derive(Debug, Clone, Copy, AnyBitPattern)]
#[repr(C)]
pub struct ElfSectionHeader64 {
    /// Offset into the section-name string table                 0-3
    pub name_offset: u32,
    /// Section type (see below)                                  4-7
    pub section_type: ElfSectionType,
    /// Section flags                                             8-15
    pub flags: ElfSectionFlags,
    /// Virtual address of the section when loaded               16-23
    pub virtual_address: u64,
    /// File offset of the section's contents                    24-31
    pub file_offset: u64,
    /// Size of the section                                      32-39
    pub size: u64,
    /// Depends on section type (symbol table, relocation, etc.) 40-43
    pub link: u32,
    /// Extra information; meaning depends on section type       44-47
    pub info: u32,
    /// Required alignment                                       48-55
    pub alignment: u64,
    /// Size of each entry, if the section is a table            56-63
    pub entry_size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, AnyBitPattern)]
#[repr(transparent)]
pub struct ElfSectionType(pub u32);
impl ElfSectionType {
    pub const NULL: Self = Self(0);
    pub const PROGBITS: Self = Self(1);
    pub const SYMTAB: Self = Self(2);
    pub const STRTAB: Self = Self(3);
    pub const RELA: Self = Self(4);
    pub const HASH: Self = Self(5);
    pub const DYNAMIC: Self = Self(6);
    pub const NOTE: Self = Self(7);
    pub const NOBITS: Self = Self(8);
    pub const REL: Self = Self(9);
    pub const SHLIB: Self = Self(10);
    pub const DYNSYM: Self = Self(11);
}

#[derive(Debug, Clone, Copy, AnyBitPattern)]
#[repr(transparent)]
pub struct ElfSectionFlags(u64);
bitflags! {
    impl ElfSectionFlags: u64 {
        const WRITE = 1 << 0;
        const ALLOC = 1 << 1;
        const EXECUTABLE = 1 << 2;
        const MERGE = 1 << 4;
        const STRINGS = 1 << 5;
        const INFO_LINK = 1 << 6;
        const LINK_ORDER = 1 << 7;
        const OS_NONCONFORMING = 1 << 8;
        const GROUP = 1 << 9;
        const TLS = 1 << 10;
    }
}
