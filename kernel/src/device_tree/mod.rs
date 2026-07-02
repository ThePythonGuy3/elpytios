use elpytios_bootinfo::Acpi;
use log::info;

use crate::vaddr::VAddr;

mod acpi;
pub use acpi::*;

unsafe fn init_device_tree_impl(_next_v_addr: &mut VAddr, system_tables: impl Iterator<Item = Result<SystemTable, AcpiError>> + ExactSizeIterator) {
    info!("Initializing device tree: found {} system tables", system_tables.len());
    for system_table in system_tables {
        let system_table = system_table.expect("Couldn't parse system table");
        info!("\t> {system_table:?}");
    }
}

/// # Safety
/// - Only call this once in setup phase after higher-half addressing is finished.
/// - Identity-mapping must still be available.
pub unsafe fn init_device_tree(acpi: Acpi, next_v_addr: &mut VAddr) {
    match acpi {
        Acpi::Acpi(addr) => {
            let rsdp = unsafe { Rsdp::new(addr.addr() as *const Rsdp) }.expect("Couldn't parse RSDP");
            let rsdt = rsdp.rsdt().expect("Couldn't parse RSDT");
            unsafe { init_device_tree_impl(next_v_addr, rsdt.into_iter()) }
        }
        Acpi::Acpi2(addr) => {
            let xsdp = unsafe { Xsdp::new(addr.addr() as *const Xsdp) }.expect("Couldn't parse XSDP");
            let xsdt = xsdp.xsdt().expect("Couldn't parse XSDT");
            unsafe { init_device_tree_impl(next_v_addr, xsdt.into_iter()) }
        }
    };
}
