use alloc::{
    alloc::{alloc, handle_alloc_error},
    boxed::Box,
};
use core::{
    alloc::{Layout, LayoutError},
    fmt, ptr,
};

use elpytios_abi::PAGE_SIZE;
use elpytios_elf::{
    Elf64, ElfError, ElfSegmentType,
    sys::{ElfProgramFlags, ElfRela64, ElfRela64Type, ElfType},
};

use crate::{
    LOWER_HALF_ADDRESSES,
    statics::{get_virtual_map, virt_to_phys},
    task::{Task, TaskCreateError},
    vaddr::{VAddr, VFlags, VirtualMapError},
};

#[derive(Clone)]
pub enum ProcessCreateError {
    Elf(ElfError),
    Layout(LayoutError),
    VMap(VirtualMapError),
    Task(TaskCreateError),
    NonRelocatable,
    InvalidAlignment(u64),
}

impl fmt::Debug for ProcessCreateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for ProcessCreateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Elf(e) => write!(f, "Couldn't parse ELF for process creation: {e}"),
            Self::Layout(e) => write!(f, "Couldn't allocate memory for program segment: {e}"),
            Self::VMap(e) => write!(f, "Couldn't virtual-map program segment: {e}"),
            Self::Task(e) => write!(f, "{e}"),
            Self::NonRelocatable => write!(f, "ELF is non-relocatable; recompile the program with -fPIE"),
            Self::InvalidAlignment(align) => write!(f, "ELF program segment alignment isn't {PAGE_SIZE} ({align})"),
        }
    }
}

impl From<ElfError> for ProcessCreateError {
    #[inline]
    fn from(value: ElfError) -> Self {
        Self::Elf(value)
    }
}

impl From<LayoutError> for ProcessCreateError {
    #[inline]
    fn from(value: LayoutError) -> Self {
        Self::Layout(value)
    }
}

impl From<VirtualMapError> for ProcessCreateError {
    #[inline]
    fn from(value: VirtualMapError) -> Self {
        Self::VMap(value)
    }
}

impl From<TaskCreateError> for ProcessCreateError {
    #[inline]
    fn from(value: TaskCreateError) -> Self {
        Self::Task(value)
    }
}

#[repr(C, align(4096))]
pub struct Process {
    executable: [u8],
}

impl Process {
    pub fn from_elf(elf: Elf64) -> Result<(Box<Self>, Task), ProcessCreateError> {
        if !matches!(elf.prologue().elf_type, ElfType::DYNAMIC) {
            return Err(ProcessCreateError::NonRelocatable)
        }

        let mut base = u64::MAX;
        let mut top = 0;
        for segment in elf.program_segments() {
            let segment = segment?;
            let ElfSegmentType::Load(..) = segment.segment_type else { continue };
            base = base.min(segment.virtual_address);
            top = top.max(segment.virtual_address + segment.memory_size);
        }

        let layout = Layout::from_size_align((top - base) as usize, PAGE_SIZE)?;
        let this = unsafe { alloc(layout) };
        if this.is_null() {
            handle_alloc_error(layout)
        }

        let exec_addr = virt_to_phys(VAddr::new(this.addr()));

        let (virtual_map, virtual_map_phys) = unsafe { get_virtual_map().for_userspace() };
        for segment in elf.program_segments() {
            let segment = segment?;
            let ElfSegmentType::Load(slice) = segment.segment_type else { continue };

            if segment.alignment != PAGE_SIZE as u64 {
                return Err(ProcessCreateError::InvalidAlignment(segment.alignment))
            }

            unsafe {
                let offset = (segment.virtual_address - base) as usize;
                this.byte_add(offset).copy_from_nonoverlapping(slice.as_ptr(), slice.len());
                this.byte_add(offset + slice.len())
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
                    let rela = this.cast::<ElfRela64>().byte_add(offset - base as usize).add(i).read_unaligned();
                    match rela.info.kind {
                        ElfRela64Type::X86_64_NONE => {}
                        ElfRela64Type::X86_64_RELATIVE => {
                            let slide = LOWER_HALF_ADDRESSES.start.addr() as i64 - base.cast_signed();
                            let patch_addr = this.add(rela.offset as usize - base as usize);
                            let value = slide + rela.addend;
                            patch_addr.cast::<i64>().write(value);
                        }
                        kind => panic!("Unsupported Elf64_Rela kind: {}", kind.0),
                    }
                }
            }
        }

        let addr_start = LOWER_HALF_ADDRESSES
            .start
            .byte_add(((top - base) as usize).next_multiple_of(PAGE_SIZE) + PAGE_SIZE);

        unsafe {
            let entry = LOWER_HALF_ADDRESSES.start.byte_add((elf.program_entry() - base) as usize);
            let this = Box::from_raw(ptr::from_raw_parts_mut(this, layout.size()));
            let task = Task::new(addr_start, entry, virtual_map, virtual_map_phys)?;
            Ok((this, task))
        }
    }
}
