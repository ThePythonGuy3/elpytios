cfg_select! {
    target_arch = "x86_64" => {
        mod x86_64;
        use x86_64 as imp;
    }
    _ => {
        compile_error!("Unsupported architecture");
    }
}

use elpytios_bootinfo::{BootInfo, DeviceTree};
use log::info;

use crate::vaddr::VAddr;

mod acpi;
use acpi::*;

unsafe fn init_device_tree_impl(
    info: &BootInfo,
    next_v_addr: &mut VAddr,
    system_tables: impl Iterator<Item = Result<SystemTable, AcpiError>> + ExactSizeIterator,
) {
    info!("Initializing device tree: found {} system tables", system_tables.len());
    macro_rules! tables {
        ($($output:ident: $type:ty;)*) => {
            $(let mut $output = None::<$type>;)*
            for system_table in system_tables {
                let system_table = system_table.expect("Couldn't parse system table");
                $(match system_table.typed::<$type>() {
                    Ok(_table) => {
                        if $output.replace(_table).is_some() {
                            panic!("Duplicate '{}' entries", SignatureFmt(&<$type as TypedSystemTable>::SIGNATURE));
                        }
                    }
                    Err(AcpiError::InvalidTableSignature { .. }) => {}
                    Err(e) => panic!("Couldn't parse typed system table: {e}"),
                })*
            }
            $(let $output = $output.unwrap_or_else(|| panic!("Missing '{}' entries", SignatureFmt(&<$type as TypedSystemTable>::SIGNATURE)));)*
        };
    }

    tables! {
        madt: Madt;
    }

    for pic in madt {
        unsafe {
            match pic {
                Pic::ProcessorLocal(proc) if (*&raw const proc.apic_flags).contains(LocalApicFlags::ENABLED) => {
                    info!("Waking up {proc:#?}");
                }
                Pic::ProcessLocalX2(proc) if (*&raw const proc.flags).contains(LocalApicFlags::ENABLED) => {}
                _ => {}
            }
        }
    }
}

/// # Safety
/// - Only call this once in setup phase after higher-half addressing is finished.
/// - Identity-mapping must still be available.
pub unsafe fn init_device_tree(info: &BootInfo, next_v_addr: &mut VAddr) {
    match info.device_tree {
        DeviceTree::Acpi(addr) => {
            let rsdp = unsafe { Rsdp::new(addr.addr() as *const Rsdp) }.expect("Couldn't parse RSDP");
            let rsdt = rsdp.rsdt().expect("Couldn't parse RSDT");
            unsafe { init_device_tree_impl(info, next_v_addr, rsdt.into_iter()) }
        }
        DeviceTree::Acpi2(addr) => {
            let xsdp = unsafe { Xsdp::new(addr.addr() as *const Xsdp) }.expect("Couldn't parse XSDP");
            let xsdt = xsdp.xsdt().expect("Couldn't parse XSDT");
            unsafe { init_device_tree_impl(info, next_v_addr, xsdt.into_iter()) }
        }
    };
}
