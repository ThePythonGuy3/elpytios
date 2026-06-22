//! A simple ELF program loader.
//!
//! ```
//! pub fn load_kernel_code() -> Result<(), ElfError> {
//!     let kernel_code = include_bytes!("../Cargo.toml");
//!
//!     match Elf::from_bytes(kernel_code)? {
//!        Elf::N32 => unreachable!("kernel ELF is 64-bits, silly"),
//!        Elf::N64(elf) => {
//!          for segment in elf {
//!               let segment = segment?;
//!
//!                 segment.segment_type;     // `ElfSegmentType`: null, load, dynamic, interp, and note
//!                 segment.segment_data;     // `&[u8]`, program segment data
//!                 segment.flags;            // `ElfProgramFlags`: 1 = executable, 2 = writable, 4 = readable
//!              segment.virtual_address;  // `usize`, virtual address that `segment_data` should be copied into
//!                 segment.physical_address; // `usize`, physical address that `segment_data` could be copied into, usually ignored
//!              segment.memory_size;      // `usize`, space size allocated in `virtual_address` (can be > `segment_data.len()`), zero-initialized
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
    const_convert,
    const_destruct,
    const_iter,
    const_option_ops,
    const_trait_impl,
    const_try,
    const_try_residual,
    try_blocks
)]

pub mod sys;

use core::{any::type_name, fmt, iter::FusedIterator, marker::Destruct};

use bytemuck::AnyBitPattern;
use const_panic::PanicFmt;
use sys::{ElfHeader64, ElfHeaderPrologue, ElfProgramFlags, ElfProgramHeader64};

#[derive(Debug, Clone, Copy, PanicFmt)]
pub enum ElfError {
    InvalidMagic([u8; 4]),
    InvalidArch(u8),
    InvalidEndian(u8),
    InvalidSegmentType(u32),
    IntDoesntFit,
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
    N32,
    N64(Elf64<'a>),
}

const impl Clone for Elf<'_> {
    fn clone(&self) -> Self {
        match self {
            Self::N32 => Self::N32,
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
}

impl<'a> Elf64<'a> {
    const fn from_bytes(file_reader: Reader<'a>, mut header_reader: Reader<'a>) -> Result<Self, ElfError> {
        let header = header_reader.read::<ElfHeader64>().ok_or(ElfError::Eof)?;
        let program_table_reader = file_reader.fork(int_fit(header.program_header_table_offset)?).ok_or(ElfError::Eof)?;

        Ok(Self {
            header,
            file_reader,
            program_table_reader,
        })
    }

    pub const fn program_entry(&self) -> u64 {
        self.header.program_entry_offset
    }

    pub const fn program_header_count(&self) -> usize {
        self.header.program_header_entry_len as usize
    }
}

const impl Clone for Elf64<'_> {
    fn clone(&self) -> Self {
        Self {
            header: self.header,
            file_reader: self.file_reader.clone(),
            program_table_reader: self.program_table_reader.clone(),
        }
    }
}

const impl<'a> Iterator for Elf64<'a> {
    type Item = Result<ElfSegment<'a>, ElfError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.header.program_header_entry_len = self.header.program_header_entry_len.checked_sub(1)?;

        let program_header = self.program_table_reader.clone().read::<ElfProgramHeader64>()?;
        self.program_table_reader.take(self.header.program_header_entry_size as usize);

        Some(try {
            let segment_data = self
                .file_reader
                .fork(int_fit(program_header.segment_offset)?)
                .ok_or(ElfError::Eof)?
                .take(int_fit(program_header.segment_file_size)?)
                .ok_or(ElfError::Eof)?;

            ElfSegment {
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
                data: segment_data,
                flags: program_header.flags,
                virtual_address: int_fit(program_header.segment_virtual_address)?,
                physical_address: int_fit(program_header.segment_physical_address)?,
                memory_size: int_fit(program_header.segment_memory_size)?,
                alignment: int_fit(program_header.section_alignment)?,
            }
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.header.program_header_entry_len as usize;
        (len, Some(len))
    }
}

impl ExactSizeIterator for Elf64<'_> {
    fn len(&self) -> usize {
        self.header.program_header_entry_len as usize
    }
}

impl FusedIterator for Elf64<'_> {}

pub struct ElfSegment<'a> {
    pub segment_type: ElfSegmentType,
    pub data: &'a [u8],
    pub flags: ElfProgramFlags,
    /// `segment_data` should be copied to this v-address
    pub virtual_address: usize,
    /// Ignored on most cases, except for kernel-related barebones programs
    pub physical_address: usize,
    /// If greater than `segment_data.len()`, then zero-fill the memory
    pub memory_size: usize,
    /// virtual_address % alignment == physical_address % alignment == 0
    pub alignment: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

struct Reader<'a> {
    bytes: &'a [u8],
}

const impl Clone for Reader<'_> {
    fn clone(&self) -> Self {
        Self { bytes: self.bytes }
    }
}

impl<'a> Reader<'a> {
    const fn fork(&self, offset: usize) -> Option<Self> {
        let (.., bytes) = self.bytes.split_at_checked(offset)?;
        Some(Self { bytes })
    }

    const fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let (taken, bytes) = self.bytes.split_at_checked(count)?;
        self.bytes = bytes;
        Some(taken)
    }

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
