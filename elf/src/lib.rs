//! A simple ELF program loader.
//!
//! ```
//! pub fn load_kernel_code() -> Result<(), ElfError> {
//!     let kernel_code = include_bytes!("../Cargo.toml");
//!
//!     match Elf::from_bytes(kernel_code)? {
//!         Elf::N32(..) => unreachable!("kernel ELF is 64-bits, silly"),
//!         Elf::N64(elf) => {
//!             for segment in elf {
//!                 let segment = segment?;
//!
//!                 segment.segment_type;     // `ElfSegmentType`: null, load, dynamic, interp, and note
//!                 segment.data;             // `&[u8]`, program segment data
//!                 segment.flags;            // `ElfProgramFlags`: 1 = executable, 2 = writable, 4 = readable
//!                 segment.virtual_address;  // `usize`, virtual address that `segment_data` should be copied into
//!                 segment.physical_address; // `usize`, physical address that `segment_data` could be copied into, usually ignored
//!                 segment.memory_size;      // `usize`, space size allocated in `virtual_address` (can be > `segment_data.len()`), zero-initialized
//!                 segment.alignment;        // `usize`, ensures that `virtual_address` and `physical_address` are multiples of this value
//!             }
//!         }
//!     }
//!
//!     Ok(())
//! }
//! ```

#![no_std]
#![feature(
    const_clone,
    const_cmp,
    const_convert,
    const_destruct,
    const_index,
    const_iter,
    const_option_ops,
    const_trait_impl,
    const_try,
    const_try_residual,
    never_type,
    try_blocks
)]

pub mod sys;

use core::{any::type_name, fmt, iter::FusedIterator, marker::Destruct};

use bytemuck::AnyBitPattern;
use const_panic::PanicFmt;
use sys::{ElfHeader64, ElfHeaderPrologue, ElfProgramFlags, ElfProgramHeader64};

use crate::sys::ElfSectionHeader64;

#[derive(Debug, Clone, Copy, PanicFmt)]
pub enum ElfError {
    InvalidMagic([u8; 4]),
    InvalidArch(u8),
    InvalidEndian(u8),
    InvalidSegmentType(u32),
    IntDoesntFit,
    MissingStringTable,
    Eof,
}

#[inline]
const fn int_fit<T: [const] TryInto<usize, Error: [const] Destruct>>(from: T) -> Result<usize, ElfError> {
    match from.try_into() {
        Ok(fit) => Ok(fit),
        Err(..) => Err(ElfError::IntDoesntFit),
    }
}

#[derive(Debug)]
pub enum Elf<'a> {
    N32(!),
    N64(Elf64<'a>),
}

const impl Clone for Elf<'_> {
    fn clone(&self) -> Self {
        match self {
            #[expect(unreachable_code, reason = "32-bit ELF isn't implemented yet")]
            Self::N32(elf) => Self::N32(elf.clone()),
            Self::N64(elf) => Self::N64(elf.clone()),
        }
    }
}

impl<'a> Elf<'a> {
    pub const fn from_bytes(bytes: &'a [u8]) -> Result<Self, ElfError> {
        let file_reader = Reader { bytes };
        let mut header_reader = file_reader.clone();

        let prologue = header_reader.read::<ElfHeaderPrologue>().ok_or(ElfError::Eof)?;
        let [0x7F, b'E', b'L', b'F'] = prologue.magic else { return Err(ElfError::InvalidMagic(prologue.magic)) };

        match prologue.arch {
            1 => panic!("32-bit ELF isn't supported yet"),
            2 => Ok(Self::N64(Elf64::from_bytes(file_reader, header_reader)?)),
            arch => Err(ElfError::InvalidArch(arch)),
        }
    }
}

#[derive(Debug)]
pub struct Elf64<'a> {
    header: ElfHeader64,
    file_reader: Reader<'a>,
    program_table_reader: Reader<'a>,
    section_table_reader: Reader<'a>,
    string_table: ElfSection64<'a>,
}

impl<'a> Elf64<'a> {
    const fn from_bytes(file_reader: Reader<'a>, mut header_reader: Reader<'a>) -> Result<Self, ElfError> {
        let header = header_reader.read::<ElfHeader64>().ok_or(ElfError::Eof)?;
        let program_table_reader = file_reader.fork(int_fit(header.program_header_table_offset)?).ok_or(ElfError::Eof)?;
        let section_table_reader = file_reader.fork(int_fit(header.section_header_table_offset)?).ok_or(ElfError::Eof)?;

        let string_table;
        let mut sections = Elf64Sections {
            len: header.section_header_entry_len,
            stride: header.section_header_entry_size,
            file_reader: file_reader.clone(),
            section_table_reader: section_table_reader.clone(),
        };

        let mut i = 0;
        loop {
            let Some(section) = sections.next() else { return Err(ElfError::MissingStringTable) };
            if i == header.section_header_string_table_index {
                string_table = section?;
                break
            } else {
                i += 1
            }
        }

        Ok(Self {
            header,
            file_reader,
            program_table_reader,
            section_table_reader,
            string_table,
        })
    }

    #[inline]
    pub const fn program_entry(&self) -> u64 {
        self.header.program_entry_offset
    }

    #[inline]
    pub const fn program_header_count(&self) -> usize {
        self.header.program_header_entry_len as usize
    }

    #[inline]
    pub const fn program_segments(&self) -> Elf64Programs<'_> {
        Elf64Programs {
            len: self.header.program_header_entry_len,
            stride: self.header.program_header_entry_size,
            file_reader: self.file_reader.clone(),
            program_table_reader: self.program_table_reader.clone(),
        }
    }

    #[inline]
    pub const fn sections(&self) -> Elf64Sections<'_> {
        Elf64Sections {
            len: self.header.section_header_entry_len,
            stride: self.header.section_header_entry_size,
            file_reader: self.file_reader.clone(),
            section_table_reader: self.section_table_reader.clone(),
        }
    }
}

const impl Clone for Elf64<'_> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            header: self.header,
            file_reader: self.file_reader.clone(),
            program_table_reader: self.program_table_reader.clone(),
            section_table_reader: self.section_table_reader.clone(),
            string_table: self.string_table.clone(),
        }
    }
}

#[derive(Debug)]
pub struct Elf64Programs<'a> {
    len: u16,
    stride: u16,
    file_reader: Reader<'a>,
    program_table_reader: Reader<'a>,
}

const impl<'a> Iterator for Elf64Programs<'a> {
    type Item = Result<ElfSegment64<'a>, ElfError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.len = self.len.checked_sub(1)?;

        let program_header = self.program_table_reader.clone().read::<ElfProgramHeader64>()?;
        self.program_table_reader.take(self.stride as usize);

        Some(try {
            let data = self
                .file_reader
                .fork(int_fit(program_header.segment_offset)?)
                .ok_or(ElfError::Eof)?
                .take(int_fit(program_header.segment_file_size)?)
                .ok_or(ElfError::Eof)?;

            ElfSegment64 {
                segment_type: match program_header.segment_type {
                    0 => ElfSegmentType::Null,
                    1 => ElfSegmentType::Load,
                    2 => ElfSegmentType::Dynamic,
                    3 => ElfSegmentType::Interp,
                    4 => ElfSegmentType::Note,
                    5 => ElfSegmentType::Shlib,
                    6 => ElfSegmentType::Header,
                    7 => ElfSegmentType::Tls,
                    n => ElfSegmentType::Unknown(n),
                },
                data,
                flags: program_header.flags,
                virtual_address: program_header.segment_virtual_address,
                physical_address: program_header.segment_physical_address,
                memory_size: program_header.segment_memory_size,
                alignment: program_header.section_alignment,
            }
        })
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len as usize;
        (len, Some(len))
    }
}

impl ExactSizeIterator for Elf64Programs<'_> {
    #[inline]
    fn len(&self) -> usize {
        self.len as usize
    }
}

impl FusedIterator for Elf64Programs<'_> {}

#[derive(Debug, Clone, Copy)]
pub struct ElfSegment64<'a> {
    pub segment_type: ElfSegmentType,
    pub data: &'a [u8],
    pub flags: ElfProgramFlags,
    /// [`Self::data`] should be copied to this v-address
    pub virtual_address: u64,
    /// Ignored on most cases, except for kernel-related barebones programs
    pub physical_address: u64,
    /// If greater than `data.len()`, then zero-fill the memory
    pub memory_size: u64,
    /// virtual_address % alignment == physical_address % alignment == 0
    pub alignment: u64,
}

#[derive(Debug, Clone, Copy, Hash)]
#[repr(u32)]
pub enum ElfSegmentType {
    /// Ignore the entry
    Null = 0,
    /// Clear p_memsz bytes at p_vaddr to 0, then copy p_filesz bytes from p_offset to p_vaddr
    Load = 1,
    /// Requires dynamic linking
    Dynamic = 2,
    /// Contains a file path to an executable to use as an interpreter for the segment
    Interp = 3,
    /// Note section. There are more values, but mostly contain architecture/environment specific
    /// information, which is probably not required for the majority of ELF files
    Note = 4,
    /// Reserved/obsolete
    Shlib = 5,
    /// The program header table itself
    Header = 6,
    /// Thread-local storage
    Tls = 7,
    /// Unknown segment type
    Unknown(u32) = u32::MAX,
}

const impl Eq for ElfSegmentType {}
const impl PartialEq for ElfSegmentType {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        match (*self, *other) {
            (Self::Null, Self::Null)
            | (Self::Load, Self::Load)
            | (Self::Dynamic, Self::Dynamic)
            | (Self::Interp, Self::Interp)
            | (Self::Note, Self::Note)
            | (Self::Shlib, Self::Shlib)
            | (Self::Header, Self::Header)
            | (Self::Tls, Self::Tls) => true,
            (Self::Unknown(l), Self::Unknown(r)) if l == r => true,
            _ => false,
        }
    }
}

#[derive(Debug)]
pub struct Elf64Sections<'a> {
    len: u16,
    stride: u16,
    file_reader: Reader<'a>,
    section_table_reader: Reader<'a>,
}

const impl<'a> Iterator for Elf64Sections<'a> {
    type Item = Result<ElfSection64<'a>, ElfError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.len = self.len.checked_sub(1)?;

        let section_header = self.section_table_reader.clone().read::<ElfSectionHeader64>()?;
        self.section_table_reader.take(self.stride as usize);

        Some(try {
            let data = self
                .file_reader
                .fork(int_fit(section_header.file_offset)?)
                .ok_or(ElfError::Eof)?
                .take(int_fit(section_header.size)?)
                .ok_or(ElfError::Eof)?;

            ElfSection64 {
                name_offset: section_header.name_offset as usize,
                data,
                virtual_address: section_header.virtual_address,
            }
        })
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len as usize;
        (len, Some(len))
    }
}

impl ExactSizeIterator for Elf64Sections<'_> {
    #[inline]
    fn len(&self) -> usize {
        self.len as usize
    }
}

impl FusedIterator for Elf64Sections<'_> {}

#[derive(Debug, Copy)]
pub struct ElfSection64<'a> {
    pub name_offset: usize,
    pub data: &'a [u8],
    pub virtual_address: u64,
}

impl<'a> ElfSection64<'a> {
    #[inline]
    pub const fn name(&self, elf: &Elf64<'a>) -> &'a [u8] {
        let start = self.name_offset;
        let mut end = start;

        while end < elf.string_table.data.len() && elf.string_table.data[end] != 0 {
            end += 1
        }

        &elf.string_table.data[start..end]
    }
}

const impl Clone for ElfSection64<'_> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            name_offset: self.name_offset,
            data: self.data,
            virtual_address: self.virtual_address,
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
}

const impl Clone for Reader<'_> {
    #[inline]
    fn clone(&self) -> Self {
        Self { bytes: self.bytes }
    }
}

impl<'a> Reader<'a> {
    #[inline]
    const fn fork(&self, offset: usize) -> Option<Self> {
        let (.., bytes) = self.bytes.split_at_checked(offset)?;
        Some(Self { bytes })
    }

    #[inline]
    const fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let (taken, bytes) = self.bytes.split_at_checked(count)?;
        self.bytes = bytes;
        Some(taken)
    }

    #[inline]
    const fn read<T: AnyBitPattern>(&mut self) -> Option<T> {
        let taken = self.take(size_of::<T>())?;
        Some(unsafe { (taken.as_ptr() as *const T).read_unaligned() })
    }
}

impl fmt::Debug for Reader<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({} bytes left)", type_name::<Self>(), self.bytes.len())
    }
}
