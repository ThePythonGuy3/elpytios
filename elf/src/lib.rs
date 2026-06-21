#![no_std]

use bytemuck::{AnyBitPattern, pod_read_unaligned};

use crate::sys::{ElfHeader64, ElfHeaderPrologue};

pub mod sys;

pub struct Elf<'a> {
    //
}

impl<'a> Elf<'a> {
    pub fn from_bytes(bytes: &'a [u8]) -> Result<Self, ElfError> {
        let mut reader = Reader::new(bytes);

        let prologue = reader.read::<ElfHeaderPrologue>()?;
        let [0x7F, b'E', b'L', b'F'] = prologue.magic else { return Err(ElfError::InvalidMagic(prologue.magic)) };
        let little_endian = match prologue.endian {
            1 => true,
            2 => false,
            endian => Err(ElfError::InvalidEndian(endian))?,
        };

        match prologue.arch {
            1 => unimplemented!("32-bit ELF isn't supported yet"),
            2 => {
                let header = reader.read::<ElfHeader64>()?;
            }
            arch => Err(ElfError::InvalidArch(arch))?,
        }

        todo!()
    }
}

pub enum ElfError {
    InvalidMagic([u8; 4]),
    InvalidArch(u8),
    InvalidEndian(u8),
    Eof,
}

struct Reader<'a> {
    start: *const u8,
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            start: bytes.as_ptr(),
            bytes,
        }
    }

    fn advance(&mut self, count: usize) -> Result<&'a [u8], ElfError> {
        let (taken, bytes) = self.bytes.split_at_checked(count).ok_or(ElfError::Eof)?;
        self.bytes = bytes;
        Ok(taken)
    }

    fn read<T: AnyBitPattern>(&mut self) -> Result<T, ElfError> {
        let taken = self.advance(size_of::<T>())?;
        Ok(pod_read_unaligned(taken))
    }
}
