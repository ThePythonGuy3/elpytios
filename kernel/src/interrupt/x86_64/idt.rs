use core::{
    arch::{asm, naked_asm},
    mem::size_of_val_raw,
};

use bitflags::bitflags;
use bytemuck::Zeroable;

use super::InterruptFrame;
use crate::{
    device::CpuContext,
    interrupt::{page_fault, x86_64::InterruptStack},
    spin_sync::SpinOnce,
};

#[derive(Copy, Clone, Zeroable)]
#[repr(C, packed)]
pub struct IdtEntry {
    pointer_low: u16,
    gdt_selector: u16,
    options: IdtOptions,
    pointer_middle: u16,
    pointer_high: u32,
    reserved: u32,
}

impl IdtEntry {
    #[inline]
    pub unsafe fn new(handler: unsafe extern "sysv64" fn() -> !, stack: InterruptStack) -> Self {
        let addr = handler as usize;
        Self {
            pointer_low: addr as u16,
            pointer_middle: (addr >> 16) as u16,
            pointer_high: (addr >> 32) as u32,
            gdt_selector: 0x08, // `KERNEL_CODE` selector
            options: IdtOptions(
                IdtOptions::PRESENT.0 | IdtOptions::DPL_RING_0.0 | IdtOptions::TYPE_INTERRUPT.0 | (stack as u16) & IdtOptions::IST_MASK.0,
            ),
            reserved: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, Zeroable)]
#[repr(transparent)]
pub struct IdtOptions(u16);
bitflags! {
    impl IdtOptions: u16 {
        const PRESENT        = 1 << 15;

        const IST_MASK       = 7;

        const DPL_RING_0     = 0 << 12;
        const DPL_RING_3     = 3 << 12;

        const TYPE_INTERRUPT = 0xe << 8;
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum IdtIndex {
    // Hard-coded by CPU
    DoubleFault = 0x08,
    PageFault = 0x0e,
    // Programmable interrupts; must send End-of-Interrupt before returning
    ScheduleTimer = 0x20,
    // Spurious vector
    Spurious = 0xff,
}

#[macro_export]
macro_rules! interrupt {
    (#[$($has_error:tt)*] $handle:ident) => {
        {
            const _: unsafe extern "sysv64" fn(&interrupt!(type => #[$($has_error)*])) = $handle;
            naked_asm!(
                r#"
                callq {save}
                callq {handle}
                callq {load}
                iretq
                "#,

                save = sym <interrupt!(type => #[$($has_error)*])>::save,
                handle = sym $handle,
                load = sym <interrupt!(type => #[$($has_error)*])>::load,

                options(att_syntax),
            )
        }
    };
    (type => #[error]) => {
        InterruptFrame<u64>
    };
    (type => #[not(error)]) => {
        InterruptFrame<()>
    };
}

#[unsafe(naked)]
pub unsafe extern "sysv64" fn int_double_fault() -> ! {
    unsafe extern "sysv64" fn handle(frame: &InterruptFrame<u64>) {
        panic!("Double-fault caught (Hardware error code: {})", frame.error)
    }

    interrupt!(
        #[error]
        handle
    )
}

#[unsafe(naked)]
pub unsafe extern "sysv64" fn int_page_fault() -> ! {
    #[repr(transparent)]
    struct ErrorCode(u64);
    bitflags! {
        impl ErrorCode: u64 {
            // 0=protection violation, 1=not present
            const NOT_PRESENT = 1 << 0;
            // 0=caused by read, 1=caused by read
            const IS_WRITE    = 1 << 1;
            // 0=triggered in ring 0, 1=triggered in ring 3
            const IS_USER     = 1 << 2;
            // overwrote reserved bits in page table
            const RESERVED    = 1 << 3;
            // instruction fetch violation
            const EXECUTE     = 1 << 4;
        }
    }

    unsafe extern "sysv64" fn handle(frame: &InterruptFrame<u64>) {
        unsafe {
            let ptr: *mut ();
            asm!("mov {ptr}, cr2", ptr = out(reg) ptr);

            let code = ErrorCode(frame.error);
            page_fault(
                ptr,
                code.contains(ErrorCode::NOT_PRESENT),
                code.contains(ErrorCode::IS_WRITE),
                code.contains(ErrorCode::IS_USER),
                code.contains(ErrorCode::RESERVED),
                code.contains(ErrorCode::EXECUTE),
            )
        }
    }

    interrupt!(
        #[error]
        handle
    )
}

#[unsafe(naked)]
pub unsafe extern "sysv64" fn schedule_timer() -> ! {
    unsafe extern "sysv64" fn handle(_frame: &InterruptFrame) {
        let cpu = CpuContext::get();
        if let Some(func) = cpu.timer_callback.get() {
            func()
        }

        unsafe { cpu.end_of_interrupt() }
    }

    interrupt!(
        #[not(error)]
        handle
    )
}

#[unsafe(naked)]
pub unsafe extern "sysv64" fn spurious() -> ! {
    naked_asm!("iretq", options(att_syntax))
}

pub unsafe fn init_idt() {
    static mut IDT_ENTRIES: [IdtEntry; 256] = [bytemuck::zeroed(); 256];
    static IDT_INIT: SpinOnce = SpinOnce::new();

    IDT_INIT.call_once(|| unsafe {
        IDT_ENTRIES[IdtIndex::DoubleFault as usize] = IdtEntry::new(int_double_fault, InterruptStack::DoubleFault);
        IDT_ENTRIES[IdtIndex::PageFault as usize] = IdtEntry::new(int_page_fault, InterruptStack::Task);

        IDT_ENTRIES[IdtIndex::ScheduleTimer as usize] = IdtEntry::new(schedule_timer, InterruptStack::Task);
        IDT_ENTRIES[IdtIndex::Spurious as usize] = IdtEntry::new(spurious, InterruptStack::Task);
    });

    #[repr(C, packed)]
    struct IdtPointer {
        limit: u16,
        base: *mut IdtEntry,
    }

    unsafe {
        let ptr = IdtPointer {
            limit: u16::try_from(size_of_val_raw(&raw const IDT_ENTRIES) - 1).unwrap(),
            base: (&raw mut IDT_ENTRIES).cast(),
        };

        asm!(
            "lidt ({ptr})",
            ptr = in(reg) &ptr,

            options(att_syntax),
        );
    }
}
