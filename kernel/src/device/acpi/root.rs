use core::{any::type_name, fmt, iter::FusedIterator};

use crate::device::{
    AcpiResult, SystemTable, SystemTableHeader,
    acpi::{AcpiParse, PackedPtr, TypedSystemTable, UnalignedPtrIter},
};

#[derive(Clone, Copy)]
pub struct Rsdt<'root> {
    entries: PackedPtr<'root>,
}

impl fmt::Debug for Rsdt<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct(type_name::<Self>())
            .field_with("entries", |f| f.debug_list().entries(self.into_iter()).finish())
            .finish()
    }
}

impl<'root> IntoIterator for Rsdt<'root> {
    type IntoIter = impl Iterator<Item = AcpiResult<SystemTable<'root>>> + ExactSizeIterator + FusedIterator + 'root;
    type Item = <Self::IntoIter as Iterator>::Item;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        UnalignedPtrIter::<u32>::new(self.entries).map(|addr| unsafe { SystemTable::parse(addr as usize as *const SystemTableHeader) })
    }
}

unsafe impl TypedSystemTable for Rsdt<'_> {
    const SIGNATURE: [u8; 4] = *b"RSDT";
    type Out<'root> = Rsdt<'root>;

    #[inline]
    unsafe fn from_table<'root>(table: &SystemTable<'root>) -> Rsdt<'root> {
        Rsdt {
            entries: PackedPtr::new(table.entries),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Xsdt<'root> {
    entries: PackedPtr<'root>,
}

impl fmt::Debug for Xsdt<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct(type_name::<Self>())
            .field_with("entries", |f| f.debug_list().entries(self.into_iter()).finish())
            .finish()
    }
}

impl<'root> IntoIterator for Xsdt<'root> {
    type IntoIter = impl Iterator<Item = AcpiResult<SystemTable<'root>>> + ExactSizeIterator + FusedIterator + 'root;
    type Item = <Self::IntoIter as Iterator>::Item;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        UnalignedPtrIter::<u64>::new(self.entries).map(|addr| unsafe { SystemTable::parse(addr as usize as *const SystemTableHeader) })
    }
}

unsafe impl TypedSystemTable for Xsdt<'_> {
    const SIGNATURE: [u8; 4] = *b"XSDT";
    type Out<'root> = Xsdt<'root>;

    #[inline]
    unsafe fn from_table<'root>(table: &SystemTable<'root>) -> Xsdt<'root> {
        Xsdt {
            entries: PackedPtr::new(table.entries),
        }
    }
}
