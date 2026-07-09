mod imp {
    cfg_select! {
        target_arch = "x86_64" => {
            mod x86_64;
            pub use x86_64::*;
        }
        _ => {
            compile_error!("Unsupported architecture");
        }
    }
}
use elpytios_bootinfo::DeviceTree;
pub use imp::CpuContext;
use log::info;

mod acpi;
use acpi::*;

use crate::ScratchPages;

unsafe fn init_device_tree_impl(
    scratch_pages: &mut ScratchPages,
    processor_entry: impl FnOnce(u32) -> ! + Clone + Send,
    system_tables: impl IntoIterator<IntoIter: ExactSizeIterator, Item = Result<SystemTable, AcpiError>>,
) -> ! {
    let system_tables = system_tables.into_iter();
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

    unsafe { imp::init_device_tree(scratch_pages, processor_entry, madt) }
}

/// # Safety
/// - Only call this once in setup phase after higher-half addressing is finished.
/// - Identity-mapping must still be available.
pub unsafe fn init_device_tree(
    device_tree: DeviceTree,
    scratch_pages: &mut ScratchPages,
    processor_entry: impl FnOnce(u32) -> ! + Clone + Send,
) -> ! {
    match device_tree {
        DeviceTree::Acpi(addr) => {
            let rsdp = unsafe { Rsdp::new(addr.addr() as *const Rsdp) }.expect("Couldn't parse RSDP");
            let rsdt = rsdp.rsdt().expect("Couldn't parse RSDT");
            unsafe { init_device_tree_impl(scratch_pages, processor_entry, rsdt) }
        }
        DeviceTree::Acpi2(addr) => {
            let xsdp = unsafe { Xsdp::new(addr.addr() as *const Xsdp) }.expect("Couldn't parse XSDP");
            let xsdt = xsdp.xsdt().expect("Couldn't parse XSDT");
            unsafe { init_device_tree_impl(scratch_pages, processor_entry, xsdt) }
        }
    };
}
