use elpytios_bootinfo::{PAGE_SIZE, paddr::PAddr};

use crate::{
    device_tree::acpi::Madt,
    interrupt::init_interrupts,
    statics::{get_phys_alloc, get_virtual_map, phys_to_virt},
    vaddr::VFlags,
};

pub unsafe fn init_device_tree(madt: Madt) {
    let v_map = get_virtual_map();
    unsafe {
        init_interrupts();

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
    }
}
