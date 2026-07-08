use core::arch::x86_64::__cpuid_count;

use elpytios_bootinfo::{PAGE_SIZE, paddr::PAddr};
use log::info;

use crate::{
    ScratchPages,
    arch::x86_64::{Msr, rdmsr, wrmsr},
    device_tree::acpi::Madt,
    interrupt::init_interrupts,
    statics::{get_phys_alloc, get_virtual_map, phys_to_virt},
    vaddr::VFlags,
};

enum ApicDriver {
    XApic { mmr: *mut u32 },
    X2Apic,
}

pub unsafe fn init_device_tree(scratch_pages: &mut ScratchPages, madt: Madt) {
    unsafe { init_interrupts() }
    let v_map = get_virtual_map();

    let driver = unsafe {
        if __cpuid_count(0x01, 0x00).ecx & (1 << 21) != 0 {
            info!("x2APIC is supported on this hardware; using Model-Specific Registers for APIC");

            wrmsr(Msr::Ia32ApicBase, rdmsr(Msr::Ia32ApicBase) | (1 << 10) | (1 << 11));
            ApicDriver::X2Apic
        } else {
            info!("x2APIC is unsupported on this hardware; falling back to legacy memory-mapped xAPIC");
            let mmr_phys = PAddr::new(madt.local_interrupt_control_addr as usize);
            let mmr = phys_to_virt(mmr_phys);

            v_map
                .map(
                    mmr_phys,
                    mmr,
                    1,
                    VFlags::WRITABLE | VFlags::WRITE_THROUGH | VFlags::CACHE_DISABLED | VFlags::EXECUTE_DISABLE,
                    || {
                        get_phys_alloc()
                            .lock()
                            .alloc(1)
                            .ok()
                            .map(|id| id.addr())
                            .inspect(|&addr| phys_to_virt(addr).ptr_mut::<u8>().write_bytes(0, PAGE_SIZE))
                    },
                )
                .unwrap();
            ApicDriver::XApic { mmr: mmr.ptr_mut::<u32>() }
        }
    };
}
