use alloc::{
    alloc::{alloc_zeroed, handle_alloc_error},
    boxed::Box,
};
use core::{
    arch::{asm, naked_asm},
    cell::UnsafeCell,
    ptr,
};

use crate::{
    arch::x86_64::{ExtendedRegisterMask, ExtendedRegisters},
    device::CpuContext,
    interrupt::x86_64::InterruptFrame,
    task::TASK_QUEUE,
    vaddr::VAddr,
};

pub struct Task {
    curr_kernel_stack: *mut u8,
    curr_schedule_inst: *const u8,
    register_mask: ExtendedRegisterMask,
    registers: Box<UnsafeCell<ExtendedRegisters>>,
}

impl Task {
    pub unsafe fn new(entry: VAddr, _user_stack_top: *mut u8, user_stack_lower_top: VAddr, kernel_stack_top: *mut u8) -> Self {
        let cpu = CpuContext::get();
        unsafe {
            let registers = alloc_zeroed(cpu.registers.layout());
            if registers.is_null() {
                handle_alloc_error(cpu.registers.layout())
            }
            let registers = Box::from_raw(ptr::from_raw_parts_mut(registers, cpu.registers.layout().size()));
            let register_mask = cpu.registers.mask();

            let frame = kernel_stack_top.cast::<InterruptFrame>().sub(1);
            frame.write(InterruptFrame {
                rax: 0,
                rcx: 0,
                rdx: 0,
                rsi: 0,
                rdi: 0,
                r8: 0,
                r9: 0,
                r10: 0,
                r11: 0,
                rbx: 0,
                rbp: 0,
                r12: 0,
                r13: 0,
                r14: 0,
                r15: 0,
                error: (),
                rip: entry.addr() as u64,
                cs: 0x20 + 3,
                rflags: 0x202,
                rsp: user_stack_lower_top.addr() as u64,
                ss: 0x18 + 3,
            });

            Self {
                curr_kernel_stack: (&raw mut (*frame).rax).cast(),
                curr_schedule_inst: Self::init as *const u8,
                register_mask,
                registers,
            }
        }
    }

    /// # Safety
    /// - Call with a `jmp` instruction.
    /// - `%rsp` must point to [`InterruptFrame::rax`].
    #[unsafe(naked)]
    unsafe extern "sysv64" fn init() -> ! {
        naked_asm!(
            "callq {load}",
            "iretq",

            load = sym InterruptFrame::<()>::load,

            options(att_syntax)
        )
    }
}

pub fn schedule() {
    let cpu = CpuContext::get();
    unsafe {
        match TASK_QUEUE.pop_front() {
            Some(next) => {
                let next_v_map = next.virtual_map_phys;

                let next_kernel_stack = next.inner.curr_kernel_stack;
                let next_schedule_inst = next.inner.curr_schedule_inst;
                let next_register_mask = next.inner.register_mask;
                let next_registers_ptr = Box::as_ptr(&next.inner.registers);

                // Note: `None` means `schedule()` is *just* called, so `%rsp` still points to the bootstrap stack
                if let Some(mut curr) = cpu.current_task.get().replace(Some(next)) {
                    cpu.registers.save(curr.inner.register_mask, curr.inner.registers.get_mut());
                    asm!(
                        "movq %rsp, {stack}",
                        "leaq {ret}(%rip), {inst}",

                        stack = in(reg) &raw mut curr.inner.curr_kernel_stack,
                        inst = in(reg) &raw mut curr.inner.curr_schedule_inst,
                        ret = label {
                            // Early-return `schedule()`
                            return
                        },

                        options(att_syntax, nostack)
                    );
                }

                cpu.registers.load(next_register_mask, UnsafeCell::raw_get(next_registers_ptr));
                asm!(
                    "movq {v_map}, %cr3",
                    "movq {stack}, %rsp",
                    "jmpq *{inst}",

                    v_map = in(reg) next_v_map.addr(),
                    stack = in(reg) next_kernel_stack,
                    inst = in(reg) next_schedule_inst,

                    options(att_syntax)
                )
            }
            None => return,
        }
    }
}
