use bytemuck::AnyBitPattern;

#[derive(Debug, Clone, Copy, AnyBitPattern)]
#[repr(C)]
pub struct ElfDyn64 {
    pub tag: ElfDt64,
    pub val: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, AnyBitPattern)]
#[repr(transparent)]
pub struct ElfDt64(pub i64);
impl ElfDt64 {
    pub const NULL: Self = Self(0);
    pub const RELA: Self = Self(7);
    pub const RELASZ: Self = Self(8);
    pub const RELAENT: Self = Self(9);
}

#[derive(Debug, Clone, Copy, AnyBitPattern)]
#[repr(C)]
pub struct ElfRela64 {
    pub offset: u64,
    pub info: ElfRela64Info,
    pub addend: i64,
}

#[derive(Debug, Clone, Copy, AnyBitPattern)]
#[repr(C, align(8))]
pub struct ElfRela64Info {
    #[cfg(target_endian = "little")]
    pub kind: ElfRela64Type,
    pub index: u32,
    #[cfg(target_endian = "big")]
    pub kind: ElfRela64Type,
}

#[derive(Debug, Clone, Copy, AnyBitPattern, PartialEq, Eq)]
#[repr(transparent)]
pub struct ElfRela64Type(pub u32);
impl ElfRela64Type {
    pub const X86_64_RELATIVE: Self = Self(8);
}
