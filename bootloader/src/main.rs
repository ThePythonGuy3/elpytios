#![feature(fn_align, const_clone, const_cmp, const_convert, const_iter, const_trait_impl, custom_inner_attributes)]
#![rustfmt::skip]

#![no_std]
#![no_main]

use core::{arch::{asm, naked_asm}, mem::MaybeUninit};

use const_panic::concat_panic;
use elpytios_elf::{Elf, Elf64, ElfSegment64, ElfSegmentType, sys::ElfProgramFlags};
use elpytios_bootinfo::{BootInfo, GraphicsInfo, paddr::PAddr, vaddr::{Entry, NodeEntry, PdEntry, PdTable, PdptEntry, PdptTable, Pml4Table, PtEntry, PtTable, VAddr, VAddrInfo}};
use uefi::{Status, boot::{self, AllocateType, MemoryType}, entry, helpers, mem::memory_map::{MemoryMap, MemoryMapOwned}, proto::console::{gop::*}};

const PAGE_SIZE: usize = 4096;

static KERNEL_BINARY: Elf64 = match Elf::from_bytes(include_bytes!(concat!("../../target/x86_64-unknown-none/", cfg_select! {
    debug_assertions => "bootloader_debug",
    _ => "bootloader",
}, "/elpytios-kernel"))) {
    Ok(Elf::N32) => panic!("Expected 64-bit kernel ELF"),
    Ok(Elf::N64(elf)) => elf,
    Err(e) => concat_panic!(e),
};

const KERNEL_SEGMENTS: [ElfSegment64; KERNEL_BINARY.program_header_count()] = {
    let mut out: MaybeUninit<[ElfSegment64; _]> = MaybeUninit::uninit();
    let mut ptr = out.as_mut_ptr() as *mut ElfSegment64;

    for segment in KERNEL_BINARY.clone() {
        let segment = match segment {
            Ok(segment) => segment,
            Err(e) => concat_panic!(e),
        };

        if segment.alignment != PAGE_SIZE as u64 {
            concat_panic!("Kernel segments must be aligned to ", PAGE_SIZE, "! Found: ", segment.alignment);
        }

        unsafe {
            ptr.write(segment);
            ptr = ptr.add(1);
        }
    }

    unsafe { out.assume_init() }
};

const KERNEL_VIRTUAL_ADDRESS: [usize; 2] = const {
    let mut base = u64::MAX;
    let mut max = u64::MIN;

    let mut i = 0;
    loop {
        let segment = KERNEL_SEGMENTS[i];
        if segment.segment_type != ElfSegmentType::Load { continue }

        base = base.min(segment.virtual_address);
        max = max.max(segment.virtual_address + segment.memory_size);

        i += 1;
        if i == KERNEL_SEGMENTS.len() { break }
    }

    match (base.try_into(), max.try_into()) {
        (Ok(base), Ok(max)) => [base, max],
        _ => panic!("Integer doesn't fit"),
    }
};

const KERNEL_STACK_PAGES: usize = 4;

#[forbid(unused, reason = "These pages must absolutely be initialized by the bootloader")]
#[derive(Clone, Copy)]
#[repr(usize)]
enum KernelBootPages {
    PageTablePhys = 0,
    //PageTableVirt = 4,
    //AllocPhys = 8,
    //AllocVirt = 9,
    JumpToKernel = 4,
    BootInfo = 5,
    Stack = 6,
    Max = Self::Stack as usize + KERNEL_STACK_PAGES,
}

struct UefiInfo {
    pub memory_map:         MemoryMapOwned,
    pub pml4_phys:          PAddr,
    pub jumper_phys:        PAddr,

    pub pml4_virt:          VAddr,
    pub boot_info:          VAddr,
    pub stack:              VAddr,
    pub kernel_entry:       VAddr,
}

fn setup_uefi_and_exit() -> UefiInfo {
    let memory_map:         MemoryMapOwned;
    let pml4_phys:          PAddr;
    let jumper_phys:        PAddr;

    let pml4_virt:          VAddr;
    let boot_info:          VAddr;
    let stack:              VAddr;
    let kernel_entry:       VAddr;
    
    helpers::init().unwrap();

    // Scope for UEFI services
    let graphics_info = {
        // Graphics Info Fetching
        let graphics_output_protocol_handle = boot::get_handle_for_protocol::<GraphicsOutput>().expect("No Graphics Output Protocol");
        let mut graphics_output_protocol;
        unsafe {
            graphics_output_protocol = boot::open_protocol::<GraphicsOutput>(
                boot::OpenProtocolParams {
                    handle: graphics_output_protocol_handle,
                    agent: boot::image_handle(),
                    controller: None
                },
                boot::OpenProtocolAttributes::GetProtocol
            ).expect("Error opening Graphics Output Protocol");
        }

        let mut max_area: usize            = 0;
        let mut max_mode: Option<Mode>     = None;
        let mut info:     Option<ModeInfo> = None;
        for possible_mode in graphics_output_protocol.modes() {
            let _info = possible_mode.info();

            let (w, h) = _info.resolution();
            let area = w * h;

            if max_mode.is_none() || area > max_area {
                max_area = area;
                max_mode = Some(possible_mode);
                info = Some(*_info);
            }
        }

        let mode:      &Mode     = &max_mode.unwrap();
        let mode_info: &ModeInfo = &info.unwrap();

        graphics_output_protocol.set_mode(mode).unwrap();

        let mut frame_buffer   = graphics_output_protocol.frame_buffer();
        let frame_buffer_size  = frame_buffer.size();
        let (w, h)             = mode_info.resolution();
        let stride             = mode_info.stride();
        let pixel_format       = mode_info.pixel_format();
        let frame_buffer_ptr   = frame_buffer.as_mut_ptr();

        GraphicsInfo {
            w,
            h,
            stride,
            pixel_format: match pixel_format {
                PixelFormat::Rgb     => elpytios_bootinfo::PixelFormat::RGB_8_BIT,
                PixelFormat::Bgr     => elpytios_bootinfo::PixelFormat::BGR_8_BIT,
                PixelFormat::Bitmask => elpytios_bootinfo::PixelFormat::BIT_MASK,
                PixelFormat::BltOnly => elpytios_bootinfo::PixelFormat::BLT_ONLY
            },
            frame_buffer: frame_buffer_ptr,
            frame_buffer_size: frame_buffer_size
        }
    };

    // UEFI services dropped
    {
        let [virtual_base, virtual_max] = KERNEL_VIRTUAL_ADDRESS;
        let kernel_page_elf_count = (virtual_max - virtual_base).div_ceil(PAGE_SIZE);
        let kernel_phys_elf_ptr = boot::allocate_pages(
            AllocateType::Address(0x200000),
            MemoryType::LOADER_DATA,
            kernel_page_elf_count + KernelBootPages::Max as usize,
        ).unwrap_or_else(|_| panic!(
            "Couldn't allocate physical memory for kernel at 0x200000 for {} pages",
            kernel_page_elf_count + KernelBootPages::Max as usize,
        )).as_ptr();

        for segment in KERNEL_SEGMENTS {
            if segment.segment_type != ElfSegmentType::Load { continue }
            unsafe {
                kernel_phys_elf_ptr
                    .add(segment.virtual_address as usize - virtual_base)
                    .copy_from_nonoverlapping(segment.data.as_ptr(), segment.data.len());
                kernel_phys_elf_ptr
                    .add(segment.virtual_address as usize - virtual_base + segment.data.len())
                    .write_bytes(0, segment.memory_size as usize - segment.data.len());
            }
        }

        unsafe {
            let kernel_phys_boot_ptr = kernel_phys_elf_ptr.add(kernel_page_elf_count * PAGE_SIZE);

            let pml4_ptr = kernel_phys_boot_ptr.add(KernelBootPages::PageTablePhys as usize * PAGE_SIZE).cast::<Pml4Table>();
            let pdpt_ptr = pml4_ptr.byte_add(PAGE_SIZE).cast::<PdptTable>();
            let pd_ptr = pdpt_ptr.byte_add(PAGE_SIZE).cast::<PdTable>();
            let pt_ptr = pd_ptr.byte_add(PAGE_SIZE).cast::<PtTable>();

            let mut out_pml4_phys = {
                let tmp: usize;
                asm!("mov {}, cr3", out(reg) tmp);
                ((tmp & !0xfff) as *mut Pml4Table).read()
            };
            let mut out_pdpt_phys = bytemuck::zeroed::<PdptTable>();
            let mut out_pd_phys = bytemuck::zeroed::<PdTable>();
            let mut out_pt_phys = bytemuck::zeroed::<PtTable>();

            let mut map = |p_addr: PAddr, v_addr: VAddr, additional_flags: Entry| {
                let VAddrInfo { pt_index, pd_index, pdpt_index, pml4_index, .. } = v_addr.indices();
                let common = Entry::PRESENT | Entry::USER_SUPERVISOR | additional_flags;

                out_pt_phys.phys_pages[pt_index] = (PtEntry::from_common(common) | PtEntry::GLOBAL).with_addr(p_addr);
                out_pd_phys.pt_entries[pd_index] = PdEntry::node(NodeEntry::from_common(common).with_addr(PAddr(pt_ptr.addr())));
                out_pdpt_phys.pd_entries[pdpt_index] = PdptEntry::node(NodeEntry::from_common(common).with_addr(PAddr(pd_ptr.addr())));
                out_pml4_phys.pdpt_entries[pml4_index] = NodeEntry::from_common(common).with_addr(PAddr(pdpt_ptr.addr()));
            };

            for segment in KERNEL_SEGMENTS {
                for i in (0..segment.memory_size as usize).step_by(PAGE_SIZE) {
                    let p_addr = PAddr(kernel_phys_elf_ptr.addr() + segment.virtual_address as usize - virtual_base + i);
                    let v_addr = VAddr::new(segment.virtual_address as usize + i);
                    map(p_addr, v_addr, (
                        match segment.flags.contains(ElfProgramFlags::WRITABLE) {
                            false => Entry::empty(),
                            true => Entry::READ_WRITE,
                        } |
                        match segment.flags.contains(ElfProgramFlags::EXECUTABLE) {
                            false => Entry::EXECUTE_DISABLE,
                            true => Entry::empty(),
                        }
                    ));
                }
            }

            for (page_kind, page_count, writable) in [
                (KernelBootPages::PageTablePhys, 4, true),
                (KernelBootPages::BootInfo, 1, false),
                (KernelBootPages::Stack, KERNEL_STACK_PAGES, true),
            ] {
                for i in 0..page_count {
                    let offset = (page_kind as usize + i) * PAGE_SIZE;
                    let p_addr = PAddr(kernel_phys_boot_ptr.addr() + offset);
                    let v_addr = VAddr::new(virtual_base + offset);
                    map(p_addr, v_addr, match writable {
                        false => Entry::empty(),
                        true => Entry::READ_WRITE,
                    });
                }
            }

            {
                // Manually identity-map `jump_to_kernel`.
                let offset = KernelBootPages::JumpToKernel as usize * PAGE_SIZE;
                let p_addr = PAddr(kernel_phys_boot_ptr.addr() + offset);
                let v_addr = VAddr::new(p_addr.0); 
                map(p_addr, v_addr, Entry::empty());
            }

            let jump_fn_addr = jump_to_kernel as *const () as usize;
            let jump_src_page = jump_fn_addr & !(PAGE_SIZE - 1);
            let jump_offset = jump_fn_addr - jump_src_page;

            let jumper_page_ptr = kernel_phys_boot_ptr.add(KernelBootPages::JumpToKernel as usize * PAGE_SIZE);
            jumper_page_ptr.copy_from_nonoverlapping(jump_src_page as *const u8, PAGE_SIZE);

            let boot_info_ptr = kernel_phys_boot_ptr.add(KernelBootPages::BootInfo as usize * PAGE_SIZE).cast::<BootInfo>();
            boot_info_ptr.write(BootInfo { graphics_info });
            pml4_ptr.write(out_pml4_phys);
            pdpt_ptr.write(out_pdpt_phys);
            pd_ptr.write(out_pd_phys);
            pt_ptr.write(out_pt_phys);

            pml4_phys = PAddr(pml4_ptr.addr());
            jumper_phys = PAddr(jumper_page_ptr.addr() + jump_offset);

            pml4_virt = VAddr::new(virtual_base + (kernel_page_elf_count + KernelBootPages::PageTablePhys as usize) * PAGE_SIZE);
            boot_info = VAddr::new(virtual_base + (kernel_page_elf_count + KernelBootPages::BootInfo as usize) * PAGE_SIZE);
            stack = VAddr::new(virtual_base + (kernel_page_elf_count + KernelBootPages::Stack as usize + KERNEL_STACK_PAGES) * PAGE_SIZE);
            kernel_entry = VAddr::new(KERNEL_BINARY.program_entry() as usize);

            memory_map = boot::exit_boot_services(Some(boot::MemoryType::LOADER_DATA));
        }
    }

    UefiInfo {
        memory_map,
        pml4_phys,
        jumper_phys,

        pml4_virt,
        boot_info,
        stack,
        kernel_entry,
    }
}

#[rustc_align(4096)]
#[unsafe(link_section = ".jump_to_kernel")]
unsafe extern "sysv64" fn jump_to_kernel(pml4_phys: usize, boot_info: usize, stack: usize, kernel_entry: usize) -> ! {
    unsafe {
        asm!(
            "mov cr3, {pml4_phys}",
            "mov rdi, {boot_info}",
            "mov rsp, {stack}",
            "jmp {kernel_entry}",

            pml4_phys = in(reg) pml4_phys,
            boot_info = in(reg) boot_info,
            stack = in(reg) stack,
            kernel_entry = in(reg) kernel_entry,

            options(noreturn)
        )
    }
    
    /*naked_asm!(
        "mov cr3, rdi", // pml4_phys
        "mov rsp, rdx", // stack
        "mov rdi, rsi", // boot_info becomes kernel arg 1
        "jmp rcx",      // kernel_entry
    )*/
}

#[entry]
fn entry() -> Status {
    let UefiInfo { memory_map, pml4_phys, jumper_phys, pml4_virt, boot_info, stack, kernel_entry } = setup_uefi_and_exit();
    unsafe {
        asm!(
            "jmp r8",

            in("r8") jumper_phys.0,
            in("rdi") pml4_phys.0,
            in("rsi") boot_info.addr(),
            in("rdx") stack.addr(),
            in("rcx") kernel_entry.addr(),

            options(noreturn)
        )
    }
}