use core::{
    arch::{asm, global_asm, x86_64::__cpuid_count},
    hint::spin_loop,
    mem::ManuallyDrop,
    sync::atomic::{
        AtomicBool, AtomicUsize,
        Ordering::{Acquire, Relaxed, Release},
    },
    time::Duration,
};

use elpytios_bootinfo::{PAGE_SIZE, paddr::PAddr};
use log::{debug, error, info};

use crate::{
    ScratchPages,
    arch::x86_64::{Msr, pit_delay, rdmsr, wrmsr},
    device_tree::acpi::{LocalApicFlags, Madt, Pic},
    interrupt::init_interrupts,
    statics::{get_phys_alloc, get_virtual_map, phys_to_virt},
    vaddr::{VAddr, VFlags},
};

global_asm!(include_str!("trampolines/x86_64.s"));
unsafe extern "sysv64" {
    static __ap_trampoline_start: u8;
    static __ap_cr3: u8;
    static __ap_cr4: u8;
    static __ap_stack: u8;
    static __ap_kernel_entry: u8;
    static __ap_kernel_arg0: u8;
    static __ap_trampoline_end: u8;
}

#[derive(Clone, Copy)]
enum ApicDriver {
    XApic { mmr: *mut u32 },
    X2Apic,
}

impl ApicDriver {
    #[inline]
    fn apic_id(self) -> u32 {
        match self {
            Self::XApic { mmr } => unsafe { (mmr.byte_add(0x20).read_volatile() >> 24) & 0xff },
            Self::X2Apic => unsafe { rdmsr(Msr::Ia32X2ApicId) as u32 },
        }
    }

    #[inline]
    unsafe fn init(self, apic_id: u32) {
        match self {
            Self::XApic { .. } => unimplemented!("Waking up cores via legacy xAPIC isn't implemented yet"),
            Self::X2Apic => unsafe {
                let id = (apic_id as u64) << 32;
                let assert = 1 << 14;

                wrmsr(Msr::Ia32X2ApicIcr, (5 << 8) | assert | id);
            },
        }
    }

    #[inline]
    unsafe fn startup(self, send_init: bool, trampoline_phys: PAddr, apic_id: u32) {
        match self {
            Self::XApic { .. } => unimplemented!("Waking up cores via legacy xAPIC isn't implemented yet"),
            Self::X2Apic => unsafe {
                // IA32_X2APIC_ICR:
                // - Bit 0-7: Vector
                // - Bit 8-10: Delivery mode (4=NMI, 5=Init, 6=Startup)
                // - Bit 14: Assert flag
                // - Bit 32-63: Target core destination APIC ID
                let id = (apic_id as u64) << 32;
                let assert = 1 << 14;

                if send_init {
                    self.init(apic_id);
                    pit_delay(Duration::from_millis(10));
                }

                wrmsr(
                    Msr::Ia32X2ApicIcr,
                    ((trampoline_phys.addr() / PAGE_SIZE) & 0xff) as u64 | (6 << 8) | assert | id,
                );
            },
        }
    }
}

pub unsafe fn init_device_tree<F: FnOnce(usize, bool) -> ! + Clone + Send>(scratch_pages: &mut ScratchPages, processor_entry: F, madt: Madt) -> ! {
    unsafe {
        init_interrupts();
        let v_map = get_virtual_map();

        let driver = if __cpuid_count(0x01, 0x00).ecx & (1 << 21) != 0 {
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
                )
                .expect("Couldn't virtual-map xAPIC MMR");
            ApicDriver::XApic { mmr: mmr.ptr_mut::<u32>() }
        };

        let trampoline_phys = scratch_pages.take().expect("Not enough scratch pages for AP trampoline entry");
        let trampoline = phys_to_virt(trampoline_phys);
        v_map
            .map(trampoline_phys, VAddr::new(trampoline_phys.addr()), 1, VFlags::empty())
            .expect("Couldn't identity-map trampoline code");
        v_map
            .map(trampoline_phys, trampoline, 1, VFlags::WRITABLE | VFlags::EXECUTE_DISABLE)
            .expect("Couldn't virtual-map trampoline code");
        let trampoline = trampoline.ptr_mut::<u8>();
        let trampoline_len = (&raw const __ap_trampoline_end).offset_from_unsigned(&raw const __ap_trampoline_start);
        assert!(
            trampoline_len <= PAGE_SIZE,
            "AP entry trampoline size {trampoline_len} is bigger than {PAGE_SIZE}"
        );

        trampoline.copy_from_nonoverlapping(&raw const __ap_trampoline_start, trampoline_len);
        trampoline
            .add((&raw const __ap_cr3).offset_from_unsigned(&raw const __ap_trampoline_start))
            .cast::<u32>()
            .write_unaligned({
                let cr3: usize;
                asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags));
                u32::try_from(cr3).expect("Page table physical address must be within 32-bit address")
            });
        trampoline
            .add((&raw const __ap_cr4).offset_from_unsigned(&raw const __ap_trampoline_start))
            .cast::<u32>()
            .write_unaligned({
                let cr4: usize;
                asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack, preserves_flags));
                u32::try_from(cr4).expect("Page table physical address must be within 32-bit address")
            });
        trampoline
            .add((&raw const __ap_kernel_entry).offset_from_unsigned(&raw const __ap_trampoline_start))
            .cast::<u64>()
            .write_unaligned(ap_kernel_entry::<F> as *const () as u64);

        debug!("\tCopied {trampoline_len} bytes into {trampoline:p} (physical address at {trampoline_phys:p}) for AP cores entry");

        static AP_INIT: AtomicBool = AtomicBool::new(false);
        static AP_PROCEED: AtomicUsize = AtomicUsize::new(0);

        unsafe extern "sysv64" fn ap_kernel_entry<F: FnOnce(usize, bool) -> ! + Clone + Send>(processor_entry: *const ()) -> ! {
            unsafe { init_interrupts() }
            let processor_entry = unsafe { (processor_entry as *const F).read() };

            AP_INIT.store(true, Release);
            let cpu_count = loop {
                match AP_PROCEED.load(Acquire) {
                    0 => {
                        spin_loop();
                        continue
                    }
                    n => break n,
                }
            };

            processor_entry(cpu_count, false)
        }

        let bsp_id = driver.apic_id();
        let mut cpu_count = 1;
        let mut init_cpu = |id: u32| {
            if bsp_id != id {
                const STACK_PAGES: usize = 16;

                let stack = get_phys_alloc()
                    .lock()
                    .alloc(STACK_PAGES.ilog2())
                    .expect("Couldn't allocate stack for AP core");
                let stack_top = phys_to_virt(stack).byte_add(STACK_PAGES * PAGE_SIZE).addr() as u64;
                let processor_entry = ManuallyDrop::new(processor_entry.clone());

                trampoline
                    .add((&raw const __ap_stack).offset_from_unsigned(&raw const __ap_trampoline_start))
                    .cast::<u64>()
                    .write_unaligned(stack_top);
                trampoline
                    .add((&raw const __ap_kernel_arg0).offset_from_unsigned(&raw const __ap_trampoline_start))
                    .cast::<u64>()
                    .write_unaligned(&raw const processor_entry as u64);

                driver.startup(true, trampoline_phys, id);
                for i in 0..2 {
                    pit_delay(Duration::from_micros(500));
                    match AP_INIT.compare_exchange(true, false, Acquire, Relaxed) {
                        Ok(..) => {
                            cpu_count += 1;
                            debug!("\tAP core {id} is up and running");
                            break
                        }
                        Err(..) => {
                            if i == 0 {
                                driver.startup(false, trampoline_phys, id);
                                error!("\tCouldn't start up AP core {id}, retrying one more time");
                            } else {
                                // Send one last INIT IPI to ensure the AP core isn't doing anything
                                driver.init(id);
                                error!("\tCouldn't start up AP core {id} even after retrying, giving up");
                            }
                        }
                    }
                }
            }
        };

        for pic in madt {
            match pic {
                Pic::ProcessorLocal(proc) if (*&raw const proc.flags).contains(LocalApicFlags::ENABLED) => {
                    init_cpu(proc.apic_id as u32);
                }
                Pic::ProcessLocalX2(proc) if (*&raw const proc.flags).contains(LocalApicFlags::ENABLED) => {
                    init_cpu(proc.x2apic_id as u32);
                }
                _ => {}
            }
        }

        AP_PROCEED.store(cpu_count, Release);
        processor_entry(cpu_count, true)
    }
}
