use core::{any::type_name, fmt, iter::FusedIterator, marker::PhantomData, mem::offset_of, num::NonZeroU8, ptr, slice};

pub const ROOT_SIGNATURE: [u8; 8] = *b"RSD PTR ";

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

    unsafe fn parse(input: *const Self::Input) -> Result<Self, AcpiError>;
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
    pub unsafe fn new(this: *const Self) -> Result<Self, AcpiError> {
        unsafe { Self::parse(this) }
    }

    #[inline]
    pub fn rsdt(&self) -> Result<SystemTableType<'_, Rsdt>, AcpiError> {
        unsafe { SystemTable::parse(self.rsdt_addr as usize as *const SystemTableHeader).and_then(SystemTable::typed) }
    }
}

impl AcpiParse for Rsdp {
    type Input = Self;

    unsafe fn parse(input: *const Self::Input) -> Result<Self, AcpiError> {
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
    pub unsafe fn new(this: *const Self) -> Result<Self, AcpiError> {
        unsafe { Self::parse(this) }
    }

    #[inline]
    pub fn xsdt(&self) -> Result<SystemTableType<'_, Xsdt>, AcpiError> {
        unsafe { SystemTable::parse(self.xsdt_addr as usize as *const SystemTableHeader).and_then(SystemTable::typed) }
    }
}

impl AcpiParse for Xsdp {
    type Input = Self;

    unsafe fn parse(input: *const Self::Input) -> Result<Self, AcpiError> {
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

    unsafe fn parse(input: *const Self::Input) -> Result<Self, AcpiError> {
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

impl<'root> SystemTable<'root> {
    #[inline]
    pub fn typed<T: sealed::TypedSystemTable>(self) -> Result<SystemTableType<'root, T>, AcpiError> {
        (self.signature == T::SIGNATURE)
            .then_some(SystemTableType {
                entries: ptr::slice_from_raw_parts(
                    self.entries as *const u8 as *const T::EntryRepr,
                    self.entries.len() / size_of::<T::EntryRepr>(),
                ),
                _marker: PhantomData,
            })
            .ok_or(AcpiError::InvalidTableSignature {
                expected: T::SIGNATURE,
                found: self.signature,
            })
    }
}

pub struct SystemTableType<'root, T: sealed::TypedSystemTable> {
    entries: *const [T::EntryRepr],
    _marker: PhantomData<&'root ()>,
}

impl<T: sealed::TypedSystemTable> Copy for SystemTableType<'_, T> {}
impl<T: sealed::TypedSystemTable> Clone for SystemTableType<'_, T> {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries,
            _marker: self._marker,
        }
    }
}

impl<'root, T: sealed::TypedSystemTable<Kind = Multiple, Entry<'root>: fmt::Debug>> fmt::Debug for SystemTableType<'root, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct(type_name::<T>())
            .field("signature", &BytesFmt(&T::SIGNATURE))
            .field_with("entries", |f| {
                let mut list = f.debug_list();
                for entry in *self {
                    _ = match entry {
                        Ok(entry) => list.entry(&entry),
                        Err(e) => list.entry_with(|f| write!(f, "[ERROR: {e}]")),
                    }
                }
                list.finish()
            })
            .finish()
    }
}

impl<'root, T: sealed::TypedSystemTable<Kind = Multiple>> Iterator for SystemTableType<'root, T> {
    type Item = Result<T::Entry<'root>, AcpiError>;

    fn next(&mut self) -> Option<Self::Item> {
        let len = self.entries.len();
        let new_len = len.checked_sub(1)?;

        let first = self.entries.cast::<T::EntryRepr>();
        let result = unsafe { T::entry(first.read_unaligned()) };

        self.entries = ptr::slice_from_raw_parts(unsafe { first.add(1) }, new_len);
        Some(result)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.entries.len(), Some(self.entries.len()))
    }
}

impl<'root, T: sealed::TypedSystemTable<Kind = Multiple>> ExactSizeIterator for SystemTableType<'root, T> {
    #[inline]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

impl<'root, T: sealed::TypedSystemTable<Kind = Multiple>> FusedIterator for SystemTableType<'root, T> {}

pub struct Rsdt;
unsafe impl sealed::TypedSystemTable for Rsdt {
    const SIGNATURE: [u8; 4] = *b"RSDT";

    type Kind = Multiple;
    type EntryRepr = u32;
    type Entry<'root> = SystemTable<'root>;

    #[inline]
    unsafe fn entry<'root>(repr: Self::EntryRepr) -> Result<Self::Entry<'root>, AcpiError> {
        unsafe { SystemTable::parse(repr as usize as *const SystemTableHeader) }
    }
}

pub struct Xsdt;
unsafe impl sealed::TypedSystemTable for Xsdt {
    const SIGNATURE: [u8; 4] = *b"XSDT";

    type Kind = Multiple;
    type EntryRepr = u64;
    type Entry<'root> = SystemTable<'root>;

    #[inline]
    unsafe fn entry<'root>(repr: Self::EntryRepr) -> Result<Self::Entry<'root>, AcpiError> {
        unsafe { SystemTable::parse(repr as usize as *const SystemTableHeader) }
    }
}

pub struct Single;
pub struct Multiple;

mod sealed {
    use super::*;

    pub unsafe trait TypedSystemTable {
        const SIGNATURE: [u8; 4];

        type Kind;
        type EntryRepr;
        type Entry<'root>;

        unsafe fn entry<'root>(repr: Self::EntryRepr) -> Result<Self::Entry<'root>, AcpiError>;
    }
}
