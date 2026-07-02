use elpytios_bootinfo::Acpi;

use crate::{statics::get_virtual_map, vaddr::VAddr};

mod acpi;
pub use acpi::*;

/// # Safety
/// - [`get_virtual_map`] must already have been set.
/// - Identity-mapping must still be available.
pub unsafe fn init_device_tree(acpi: Acpi, _next_v_addr: &mut VAddr) {
    let _v_map = get_virtual_map();
    match acpi {
        Acpi::Acpi(..) => unimplemented!("32-bit ACPI 1.0 (RSDP) not implemented yet"),
        Acpi::Acpi2(addr) => {
            let xsdp = unsafe { Xsdp::new(addr.addr() as *const Xsdp) }.expect("Couldn't parse XSDP");
            let xsdt = xsdp.xsdt().expect("Couldn't parse XSDT");
        }
    };
}
