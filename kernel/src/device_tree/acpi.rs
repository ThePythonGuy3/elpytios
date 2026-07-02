use core::{fmt, mem::offset_of, num::NonZeroU8, slice};

pub const ROOT_SIGNATURE: &'static [u8] = b"RSD PTR ";

#[derive(Clone, Copy)]
pub enum AcpiError {
    InvalidRootSignature { found: [u8; 8] },
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
            Self::InvalidRootSignature { found } => {
                write!(f, "Expected 'RSD PTR ', found '")?;
                for chunk in found.utf8_chunks() {
                    for ch in chunk.valid().chars() {
                        write!(f, "{}", ch.escape_debug())?;
                    }
                    for byte in chunk.invalid() {
                        write!(f, "\\x{:02x}", byte)?;
                    }
                }

                write!(f, "'")
            }
            Self::InvalidChecksum(checksum) => write!(f, "Invalid checksum, found {checksum}"),
        }
    }
}

pub trait AcpiParse {
    type Output;

    /// # Safety
    /// The pointer must be valid for unaligned reads of `Self`.
    unsafe fn new(self: *const Self) -> Result<Self::Output, AcpiError>;
}

#[repr(C, packed)]
pub struct Rsdp {
    signature: [u8; 8],
    checksum: u8,
    oem_id: [u8; 6],
    revision: u8,
    rsdt_addr: u32, // Note: Explicitly use `u32` because it's a 32-bit address regardless of architecture
}

impl AcpiParse for Rsdp {
    type Output = Self;

    unsafe fn new(self: *const Self) -> Result<Self::Output, AcpiError> {
        unsafe {
            ((*self).signature == ROOT_SIGNATURE).ok_or(AcpiError::InvalidRootSignature { found: (*self).signature })?;
            let mut checksum = 0u8;
            for &byte in slice::from_raw_parts(&raw const *self as *const u8, size_of::<Self>()) {
                checksum = checksum.wrapping_add(byte);
            }

            match NonZeroU8::new(checksum) {
                None => Ok(self.read_unaligned()),
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

impl AcpiParse for Xsdp {
    type Output = Self;

    unsafe fn new(self: *const Self) -> Result<Self::Output, AcpiError> {
        unsafe {
            (&raw const (*self).rsdp).new()?;

            let mut checksum = 0u8;
            for &byte in slice::from_raw_parts(&raw const (*self).len as *const u8, size_of::<Self>() - offset_of!(Self, len)) {
                checksum = checksum.wrapping_add(byte);
            }

            match NonZeroU8::new(checksum) {
                None => Ok(self.read_unaligned()),
                Some(invalid) => Err(AcpiError::InvalidChecksum(invalid)),
            }
        }
    }
}
