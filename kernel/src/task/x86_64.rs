use alloc::{
    alloc::{alloc, handle_alloc_error},
    boxed::Box,
};
use core::{alloc::Layout, arch::asm, cell::UnsafeCell, mem::offset_of, ptr};

use elpytios_bootinfo::{PAGE_SIZE, paddr::PAddr};
use elpytios_elf::{
    Elf64, ElfSegmentType,
    sys::{ElfProgramFlags, ElfRela64, ElfRela64Type, ElfType},
};

use crate::{
    LOWER_HALF_ADDRESSES,
    arch::x86_64::{ExtendedRegisterBuffer, ExtendedRegisterMask},
    device::CpuContext,
    interrupt::x86_64::InterruptFrame,
    statics::{get_phys_alloc, get_virtual_map, phys_to_virt},
    task::TaskCreateError,
    vaddr::{VFlags, VirtualMap},
};

#[repr(C)]
pub struct Task {
    frame: InterruptFrame,
    executable: usize,
    stack: usize,
    virtual_map: VirtualMap,
    virtual_map_phys: PAddr,
    register_mask: ExtendedRegisterMask,
    registers: ExtendedRegisterBuffer,
}

impl Task {
    #[inline]
    fn new() -> *mut Self {
        let register_layout = CpuContext::get().registers.layout();

        let layout = Layout::new::<InterruptFrame>(); // `frame`
        let (layout, ..) = layout.extend(Layout::new::<usize>()).unwrap(); // `executable`
        let (layout, ..) = layout.extend(Layout::new::<usize>()).unwrap(); // `stack`
        let (layout, ..) = layout.extend(Layout::new::<VirtualMap>()).unwrap(); // `virtual_map`
        let (layout, ..) = layout.extend(Layout::new::<PAddr>()).unwrap(); // `virtual_map_phys`
        let (layout, ..) = layout.extend(Layout::new::<ExtendedRegisterMask>()).unwrap(); // `register_mask`
        let (layout, ..) = layout.extend(register_layout).unwrap(); // `registers`

        let ptr = unsafe { alloc(layout.pad_to_align()) };
        if ptr.is_null() {
            handle_alloc_error(layout)
        }

        ptr::from_raw_parts_mut(ptr, register_layout.size())
    }

    pub fn from_elf(elf: Elf64) -> Result<Box<Self>, TaskCreateError> {
        if !matches!(elf.prologue().elf_type, ElfType::DYNAMIC) {
            return Err(TaskCreateError::NonRelocatable)
        }

        let mut base = u64::MAX;
        let mut top = 0;
        for segment in elf.program_segments() {
            let segment = segment?;
            let ElfSegmentType::Load(..) = segment.segment_type else { continue };
            base = base.min(segment.virtual_address);
            top = top.max(segment.virtual_address + segment.memory_size);
        }

        let exec_order = (top - base).div_ceil(PAGE_SIZE as u64).next_power_of_two().ilog2();
        let exec_addr = get_phys_alloc().lock().alloc(exec_order)?;
        let exec_ptr = phys_to_virt(exec_addr).ptr_mut::<u8>();

        let (virtual_map, virtual_map_phys) = unsafe { get_virtual_map().for_userspace() };
        for segment in elf.program_segments() {
            let segment = segment?;
            let ElfSegmentType::Load(slice) = segment.segment_type else { continue };

            if segment.alignment != PAGE_SIZE as u64 {
                return Err(TaskCreateError::InvalidAlignment(segment.alignment))
            }

            unsafe {
                let offset = (segment.virtual_address - base) as usize;
                exec_ptr.byte_add(offset).copy_from_nonoverlapping(slice.as_ptr(), slice.len());
                exec_ptr
                    .byte_add(offset + slice.len())
                    .write_bytes(0, segment.memory_size as usize - slice.len());

                virtual_map.map(
                    exec_addr.byte_add(offset),
                    LOWER_HALF_ADDRESSES.start.byte_add(offset),
                    (segment.memory_size as usize).div_ceil(PAGE_SIZE),
                    {
                        let mut flags = VFlags::USER_MODE;
                        if !segment.flags.contains(ElfProgramFlags::EXECUTABLE) {
                            flags |= VFlags::EXECUTE_DISABLE;
                        }
                        if segment.flags.contains(ElfProgramFlags::WRITABLE) {
                            flags |= VFlags::WRITABLE;
                        }
                        flags
                    },
                )?;
            }
        }

        for segment in elf.program_segments() {
            let ElfSegmentType::Dynamic { offset, size, stride } = segment?.segment_type else { continue };
            for i in 0..size / stride {
                unsafe {
                    let rela = exec_ptr.cast::<ElfRela64>().byte_add(offset - base as usize).add(i).read_unaligned();
                    match rela.info.kind {
                        ElfRela64Type::X86_64_RELATIVE => {
                            let slide = LOWER_HALF_ADDRESSES.start.addr() as i64 - base.cast_signed();
                            let patch_addr = exec_ptr.add(rela.offset as usize - base as usize);
                            let value = slide + rela.addend;
                            patch_addr.cast::<i64>().write(value);
                        }
                        kind => panic!("Unsupported Elf64_Rela kind: {}", kind.0),
                    }
                }
            }
        }

        let stack_addr = get_phys_alloc().lock().alloc(16u32.ilog2())?;
        unsafe {
            virtual_map.map(
                stack_addr.byte_add(PAGE_SIZE),
                LOWER_HALF_ADDRESSES
                    .start
                    .byte_add(((top - base) as usize).next_multiple_of(PAGE_SIZE) + PAGE_SIZE),
                15,
                VFlags::USER_MODE | VFlags::WRITABLE,
            )?;
        };

        let entry = elf.program_entry() - base;
        unsafe {
            let this = Self::new();
            (&raw mut (*this).frame).write(InterruptFrame {
                r15: 0,
                r14: 0,
                r13: 0,
                r12: 0,
                rbp: 0,
                rbx: 0,
                r11: 0,
                r10: 0,
                r9: 0,
                r8: 0,
                rdi: 0,
                rsi: 0,
                rdx: 0,
                rcx: 0,
                rax: 0,
                error: (),
                rip: LOWER_HALF_ADDRESSES.start.addr() as u64 + entry,
                cs: 0x20 + 3,
                rflags: 0x202,
                rsp: LOWER_HALF_ADDRESSES.start.addr() as u64 + (top - base).next_multiple_of(PAGE_SIZE as u64) + 16 * PAGE_SIZE as u64,
                ss: 0x18 + 3,
            });
            (&raw mut (*this).executable).write(exec_addr.addr() | exec_order as usize);
            (&raw mut (*this).stack).write(stack_addr.addr() | 16u32.ilog2() as usize);
            (&raw mut (*this).virtual_map).write(virtual_map);
            (&raw mut (*this).virtual_map_phys).write(virtual_map_phys);
            (&raw mut (*this).register_mask).write(CpuContext::get().registers.mask());

            Ok(Box::from_raw(this))
        }
    }
}

pub unsafe fn schedule_init(task: Box<Task>) -> ! {
    let cpu = CpuContext::get();
    unsafe {
        let task_ptr = Box::as_ptr(&task);
        cpu.current_task.get().write(Some(task));

        // Restore all extended registers (via xrstor64)
        cpu.registers.load((*task_ptr).register_mask, &(*task_ptr).registers);
        asm!(
            // Setup kernel stack
            "movq %rsp, (%rcx)",
            // Switch memory mapping
            "movq %rdx, %cr3",

            // Push the interrupt return data
            "pushq {ss}(%rax)",
            "pushq {rsp}(%rax)",
            "pushq {rflags}(%rax)",
            "pushq {cs}(%rax)",
            "pushq {rip}(%rax)",

            // Restore all general-purpose registers
            "movq {r15}(%rax), %r15",
            "movq {r14}(%rax), %r14",
            "movq {r13}(%rax), %r13",
            "movq {r12}(%rax), %r12",
            "movq {rbp}(%rax), %rbp",
            "movq {rbx}(%rax), %rbx",
            "movq {r11}(%rax), %r11",
            "movq {r10}(%rax), %r10",
            "movq {r9}(%rax), %r9",
            "movq {r8}(%rax), %r8",
            "movq {rdi}(%rax), %rdi",
            "movq {rsi}(%rax), %rsi",
            "movq {rdx}(%rax), %rdx",
            "movq {rcx}(%rax), %rcx",
            "movq {rax}(%rax), %rax",

            // Do interrupt return
            "swapgs",
            "iretq",

            in("rax") &raw const (*task_ptr).frame,
            in("rcx") UnsafeCell::raw_get(&raw const cpu.tss.rsp0),
            in("rdx") (*task_ptr).virtual_map_phys.addr(),

            r15 = const offset_of!(InterruptFrame, r15),
            r14 = const offset_of!(InterruptFrame, r14),
            r13 = const offset_of!(InterruptFrame, r13),
            r12 = const offset_of!(InterruptFrame, r12),
            rbp = const offset_of!(InterruptFrame, rbp),
            rbx = const offset_of!(InterruptFrame, rbx),
            r11 = const offset_of!(InterruptFrame, r11),
            r10 = const offset_of!(InterruptFrame, r10),
            r9 = const offset_of!(InterruptFrame, r9),
            r8 = const offset_of!(InterruptFrame, r8),
            rdi = const offset_of!(InterruptFrame, rdi),
            rsi = const offset_of!(InterruptFrame, rsi),
            rdx = const offset_of!(InterruptFrame, rdx),
            rcx = const offset_of!(InterruptFrame, rcx),
            rax = const offset_of!(InterruptFrame, rax),

            ss = const offset_of!(InterruptFrame, ss),
            rsp = const offset_of!(InterruptFrame, rsp),
            rflags = const offset_of!(InterruptFrame, rflags),
            cs = const offset_of!(InterruptFrame, cs),
            rip = const offset_of!(InterruptFrame, rip),

            options(att_syntax, noreturn),
        )
    }
}
