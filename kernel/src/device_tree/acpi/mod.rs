use core::{any::type_name, fmt, iter::FusedIterator, marker::PhantomData, mem::offset_of, num::NonZeroU8, ptr, slice};

mod madt;
mod root;
pub use madt::*;
pub use root::*;

pub const ROOT_SIGNATURE: [u8; 8] = *b"RSD PTR ";

pub type AcpiResult<T> = Result<T, AcpiError>;

struct BytesFmt<'a>(&'a [u8]);
impl fmt::Debug for BytesFmt<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for BytesFmt<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for chunk in self.0.utf8_chunks() {
            for ch in chunk.valid().chars() {
                write!(f, "{}", ch.escape_debug())?;
            }
            for byte in chunk.invalid() {
                write!(f, "\\x{:02x}", byte)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub enum AcpiError {
    InvalidRootSignature { found: [u8; 8] },
    InvalidTableSignature { expected: [u8; 4], found: [u8; 4] },
    InvalidChecksum(NonZeroU8),
}

impl fmt::Debug for AcpiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for AcpiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRootSignature { found } => write!(f, "Expected 'RSD PTR ', found '{}'", BytesFmt(found)),
            Self::InvalidTableSignature { expected, found } => write!(f, "Expected '{}', found '{}'", BytesFmt(expected), BytesFmt(found)),
            Self::InvalidChecksum(checksum) => write!(f, "Invalid checksum, found {checksum}"),
        }
    }
}

trait AcpiParse: Sized {
    type Input;

    unsafe fn parse(input: *const Self::Input) -> AcpiResult<Self>;
}

#[repr(C, packed)]
pub struct Rsdp {
    signature: [u8; 8],
    checksum: u8,
    oem_id: [u8; 6],
    revision: u8,
    rsdt_addr: u32, // Note: Explicitly use `u32` because it's a 32-bit address regardless of architecture
}

impl Rsdp {
    /// # Safety
    /// - The pointer must be valid for unaligned reads of `Rsdp`.
    /// - The pointer must point to a firmware-provided data to ensure validations.
    /// - The resulting output must be dropped before identity-mapping is disabled.
    #[inline]
    pub unsafe fn new(this: *const Self) -> AcpiResult<Self> {
        unsafe { Self::parse(this) }
    }

    #[inline]
    pub fn rsdt(&self) -> AcpiResult<Rsdt<'_>> {
        unsafe { SystemTable::parse(self.rsdt_addr as usize as *const SystemTableHeader).and_then(|table| table.typed()) }
    }
}

impl AcpiParse for Rsdp {
    type Input = Self;

    unsafe fn parse(input: *const Self::Input) -> AcpiResult<Self> {
        unsafe {
            ((*input).signature == ROOT_SIGNATURE).ok_or(AcpiError::InvalidRootSignature { found: (*input).signature })?;
            let mut checksum = 0u8;
            for &byte in slice::from_raw_parts(&raw const *input as *const u8, size_of::<Self>()) {
                checksum = checksum.wrapping_add(byte);
            }

            match NonZeroU8::new(checksum) {
                None => Ok(input.read_unaligned()),
                Some(invalid) => Err(AcpiError::InvalidChecksum(invalid)),
            }
        }
    }
}

#[repr(C, packed)]
pub struct Xsdp {
    rsdp: Rsdp,
    len: u32,
    xsdt_addr: u64, // Note: Explicitly use `u64` because it's a 64-bit address regardless of architecture
    ext_checksum: u8,
    _reserved: [u8; 3],
}

impl Xsdp {
    /// # Safety
    /// - The pointer must be valid for unaligned reads of `Xsdp`.
    /// - The pointer must point to a firmware-provided data to ensure validations.
    /// - The resulting output must be dropped before identity-mapping is disabled.
    #[inline]
    pub unsafe fn new(this: *const Self) -> AcpiResult<Self> {
        unsafe { Self::parse(this) }
    }

    #[inline]
    pub fn xsdt(&self) -> AcpiResult<Xsdt<'_>> {
        unsafe { SystemTable::parse(self.xsdt_addr as usize as *const SystemTableHeader).and_then(|table| table.typed()) }
    }
}

impl AcpiParse for Xsdp {
    type Input = Self;

    unsafe fn parse(input: *const Self::Input) -> AcpiResult<Self> {
        unsafe {
            Rsdp::parse(&raw const (*input).rsdp)?;

            let mut checksum = 0u8;
            for &byte in slice::from_raw_parts(&raw const (*input).len as *const u8, size_of::<Self>() - offset_of!(Self, len)) {
                checksum = checksum.wrapping_add(byte);
            }

            match NonZeroU8::new(checksum) {
                None => Ok(input.read_unaligned()),
                Some(invalid) => Err(AcpiError::InvalidChecksum(invalid)),
            }
        }
    }
}

#[repr(C, packed)]
pub struct SystemTableHeader {
    signature: [u8; 4],
    length: u32,
    revision: u8,
    checksum: u8,
    oem_id: [u8; 6],
    oem_table_id: u64,
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
    system_table: (),
}

#[derive(Clone, Copy)]
pub struct SystemTable<'root> {
    signature: [u8; 4],
    entries: *const [u8],
    _marker: PhantomData<&'root ()>,
}

impl<'root> SystemTable<'root> {
    #[inline]
    pub fn typed<T: TypedSystemTable<Out<'root> = T>>(&self) -> AcpiResult<T> {
        (self.signature == T::SIGNATURE)
            .then(|| unsafe { T::from_table(self) })
            .ok_or(AcpiError::InvalidTableSignature {
                expected: T::SIGNATURE,
                found: self.signature,
            })
    }
}

impl fmt::Debug for SystemTable<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct(type_name::<Self>())
            .field("signature", &BytesFmt(&self.signature))
            .field("byte_len", &self.entries.len())
            .finish()
    }
}

impl AcpiParse for SystemTable<'_> {
    type Input = SystemTableHeader;

    unsafe fn parse(input: *const Self::Input) -> AcpiResult<Self> {
        unsafe {
            let entries = slice::from_raw_parts(
                &raw const (*input).system_table as *const u8,
                (*input).length as usize - size_of::<SystemTableHeader>(),
            );

            let mut checksum = 0u8;
            for &byte in slice::from_raw_parts(&raw const *input as *const u8, size_of::<SystemTableHeader>()) {
                checksum = checksum.wrapping_add(byte);
            }

            for &byte in entries {
                checksum = checksum.wrapping_add(byte);
            }

            match NonZeroU8::new(checksum) {
                None => Ok(Self {
                    signature: (*input).signature,
                    entries,
                    _marker: PhantomData,
                }),
                Some(invalid) => Err(AcpiError::InvalidChecksum(invalid)),
            }
        }
    }
}

#[derive(Clone, Copy)]
struct PackedPtr<'root> {
    ptr: *const [u8],
    _marker: PhantomData<&'root ()>,
}

impl<'root> PackedPtr<'root> {
    #[inline]
    fn new(ptr: *const [u8]) -> Self {
        Self { ptr, _marker: PhantomData }
    }

    #[inline]
    fn len(self) -> usize {
        self.ptr.len()
    }

    #[inline]
    unsafe fn slice(&mut self, len: usize) -> &'root [u8] {
        unsafe {
            let new_len = self.ptr.len().checked_sub(len).expect("Not enough bytes");
            let first = self.ptr.as_ptr();
            let result = slice::from_raw_parts(first, len);

            self.ptr = ptr::slice_from_raw_parts(first.add(len), new_len);
            result
        }
    }

    #[inline]
    unsafe fn read<T: 'root>(&mut self) -> T {
        unsafe {
            let new_len = self.ptr.len().checked_sub(size_of::<T>()).expect("Not enough bytes");
            let first = self.ptr.cast::<T>();
            let result = first.read_unaligned();

            self.ptr = ptr::slice_from_raw_parts(first.add(1).cast(), new_len);
            result
        }
    }
}

struct UnalignedPtrIter<'root, T: 'root> {
    entries: PackedPtr<'root>,
    _marker: PhantomData<&'root [T]>,
}

impl<'root, T: 'root> UnalignedPtrIter<'root, T> {
    fn new(entries: PackedPtr<'root>) -> Self {
        Self {
            entries,
            _marker: PhantomData,
        }
    }
}

impl<'root, T: 'root> Iterator for UnalignedPtrIter<'root, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        (self.entries.len() != 0).then(|| unsafe { self.entries.read() })
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.entries.len() / size_of::<T>(), Some(self.entries.len() / size_of::<T>()))
    }
}

impl<'root, T: 'root> ExactSizeIterator for UnalignedPtrIter<'root, T> {
    #[inline]
    fn len(&self) -> usize {
        self.entries.len() / size_of::<T>()
    }
}

impl<'root, T: 'root> FusedIterator for UnalignedPtrIter<'root, T> {}

use sealed::*;
mod sealed {
    use super::*;

    pub unsafe trait TypedSystemTable: Sized {
        const SIGNATURE: [u8; 4];
        type Out<'root>: TypedSystemTable;

        unsafe fn from_table<'root>(table: &SystemTable<'root>) -> Self::Out<'root>;
    }
}
