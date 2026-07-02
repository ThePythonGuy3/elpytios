use core::{fmt, mem::offset_of, num::NonZeroU8, slice};

use elpytios_bootinfo::Acpi;

use crate::{statics::get_virtual_map, vaddr::VAddr};

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

#[repr(C, packed)]
pub struct Rsdp {
    pub signature: [u8; 8],
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub revision: u8,
    pub rsdt_addr: u32, // Note: Explicitly use `u32` because it's a 32-bit address regardless of architecture
}

impl Rsdp {
    #[inline]
    pub fn validate(&self) -> Result<(), AcpiError> {
        (self.signature == ROOT_SIGNATURE).ok_or(AcpiError::InvalidRootSignature { found: self.signature })?;
        unsafe {
            let mut checksum = 0u8;
            for &byte in slice::from_raw_parts(&raw const *self as *const u8, size_of::<Self>()) {
                checksum = checksum.wrapping_add(byte);
            }

            match NonZeroU8::new(checksum) {
                None => Ok(()),
                Some(invalid) => Err(AcpiError::InvalidChecksum(invalid)),
            }
        }
    }
}

#[repr(C, packed)]
pub struct Xsdp {
    pub rsdp: Rsdp,
    pub len: u32,
    pub xsdt_addr: u64, // Note: Explicitly use `u64` because it's a 64-bit address regardless of architecture
    pub ext_checksum: u8,
    pub _reserved: [u8; 3],
}

impl Xsdp {
    #[inline]
    pub fn validate(&self) -> Result<(), AcpiError> {
        self.rsdp.validate()?;
        unsafe {
            let mut checksum = 0u8;
            for &byte in slice::from_raw_parts(&raw const self.len as *const u8, size_of::<Self>() - offset_of!(Self, len)) {
                checksum = checksum.wrapping_add(byte);
            }

            match NonZeroU8::new(checksum) {
                None => Ok(()),
                Some(invalid) => Err(AcpiError::InvalidChecksum(invalid)),
            }
        }
    }
}

/// # Safety
/// - [`get_virtual_map`] must already have been set.
/// - Identity-mapping must still be available.
pub unsafe fn init_device_tree(acpi: Acpi, _next_v_addr: &mut VAddr) {
    let _v_map = get_virtual_map();
    match acpi {
        Acpi::Acpi(..) => unimplemented!("32-bit ACPI 1.0 (RSDP) not implemented yet"),
        Acpi::Acpi2(addr) => {
            let xsdp = unsafe { (addr.addr() as *const Xsdp).read_unaligned() };
            xsdp.validate().unwrap();
        }
    }
}
