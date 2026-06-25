#![feature(const_clone, const_cmp, const_convert, const_iter, const_trait_impl, custom_inner_attributes, fn_align)]
#![rustfmt::skip]

#![no_std]
#![no_main]

use core::{arch::{asm, naked_asm}, mem::MaybeUninit};

use const_panic::concat_panic;
use elpytios_elf::{Elf, Elf64, ElfSegment64, ElfSegmentType, sys::ElfProgramFlags};
use elpytios_bootinfo::{BootInfo, GraphicsInfo, MemoryRegion, paddr::PAddr, vaddr::{Entry, NodeEntry, PdEntry, PdTable, PdptEntry, PdptTable, Pml4Table, PtEntry, PtTable, UnionEntry, VAddr, VAddrInfo}};
use uefi::{Status, boot::{self, AllocateType, MemoryType}, entry, helpers, mem::memory_map::{MemoryMap, MemoryMapOwned}, proto::console::gop::*};

const PAGE_SIZE: usize = 4096;

static KERNEL_BINARY: Elf64 = match Elf::from_bytes(include_bytes!(concat!("../../target/x86_64-unknown-none/", cfg_select! {
    debug_assertions => "bootloader_debug",
    _ => "bootloader",
}, "/elpytios-kernel"))) {
    Ok(Elf::N32) => panic!("Expected 64-bit kernel ELF"),
    Ok(Elf::N64(elf)) => elf,
    Err(e) => concat_panic!(e),
};

const ELPYTI_KERNEL_CODE:  MemoryType = MemoryType::custom(0x8000_0000);
const ELPYTI_KERNEL_STACK: MemoryType = MemoryType::custom(0x8000_0001);
const ELPYTI_BOOT_INFO:    MemoryType = MemoryType::custom(0x8000_0002);
const ELPYTI_PAGE_TABLE:   MemoryType = MemoryType::custom(0x8000_0003);

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

const KERNEL_STACK_PAGES: usize = 8;

struct UefiInfo {
    pub memory_map:         MemoryMapOwned,
    pub pml4_phys:          PAddr,

    pub boot_info:          VAddr,
    pub kernel_stack_base:  VAddr,
    pub kernel_entry:       VAddr,
}

fn setup_uefi_and_exit() -> UefiInfo {
    let memory_map:         MemoryMapOwned;
    let pml4_phys:          PAddr;

    let boot_info_ptr:     *mut BootInfo;
    let boot_info:          VAddr;
    let kernel_stack_base:  VAddr;
    let kernel_entry:       VAddr;
    
    helpers::init().unwrap();

    {
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

        let graphics_info = GraphicsInfo {
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
        };

        let mut pml4 = bytemuck::zeroed::<Pml4Table>();
        fn new_page_table() -> PAddr {
            let ptr = boot::allocate_pages(AllocateType::AnyPages, ELPYTI_PAGE_TABLE, 1).unwrap().as_ptr();
            unsafe { ptr.write_bytes(0, PAGE_SIZE) }
            PAddr::new(ptr.expose_provenance())
        }

        let mut map_phys_to_virt = |p_addr: PAddr, v_addr: VAddr, flags: Entry| {
            let VAddrInfo { pt_index, pd_index, pdpt_index, pml4_index, .. } = v_addr.indices();
            unsafe {
                let pdpt = match pml4.pdpt_entries[pml4_index] {
                    e if e.is_present() => e,
                    ref mut e => {
                        *e = NodeEntry::new(Entry::WRITABLE, new_page_table());
                        *e
                    }
                }.child_addr().addr() as *mut PdptTable;

                let pd = match (*pdpt).pd_entries[pdpt_index] {
                    e if e.is_present() && let UnionEntry::Node(e) = e.kind() => e,
                    ref mut e => {
                        *e = PdptEntry::node(NodeEntry::new(Entry::WRITABLE, new_page_table()));
                        e.kind().force_node()
                    }
                }.child_addr().addr() as *mut PdTable;

                let pt = match (*pd).pt_entries[pd_index] {
                    e if e.is_present() && let UnionEntry::Node(e) = e.kind() => e,
                    ref mut e => {
                        *e = PdEntry::node(NodeEntry::new(Entry::WRITABLE, new_page_table()));
                        e.kind().force_node()
                    }
                }.child_addr().addr() as *mut PtTable;

                match (*pt).phys_pages[pt_index] {
                    e if e.is_present() => panic!("Couldn't map {v_addr} to {p_addr}; already mapped to {}", e.addr()),
                    ref mut e => *e = PtEntry::new(flags, p_addr),
                }
            }
        };

        let [kernel_virt_base, kernel_virt_max] = KERNEL_VIRTUAL_ADDRESS;
        let kernel_ptr = boot::allocate_pages(AllocateType::AnyPages, ELPYTI_KERNEL_CODE, (kernel_virt_max - kernel_virt_base).div_ceil(PAGE_SIZE)).unwrap().as_ptr();
        for segment in KERNEL_SEGMENTS {
            if segment.segment_type != ElfSegmentType::Load { continue }
            unsafe {
                kernel_ptr
                    .add(segment.virtual_address as usize - kernel_virt_base)
                    .copy_from_nonoverlapping(segment.data.as_ptr(), segment.data.len());
                kernel_ptr
                    .add(segment.virtual_address as usize - kernel_virt_base + segment.data.len())
                    .write_bytes(0, segment.memory_size as usize - segment.data.len());
            }

            for i in (0..segment.memory_size as usize).step_by(PAGE_SIZE) {
                map_phys_to_virt(
                    PAddr::new(kernel_ptr.expose_provenance() + segment.virtual_address as usize - kernel_virt_base + i),
                    VAddr::new(segment.virtual_address as usize + i),
                    match segment.flags.contains(ElfProgramFlags::WRITABLE) {
                        false => Entry::empty(),
                        true => Entry::WRITABLE,
                    },
                );
            }
        }
        kernel_entry = VAddr::new(KERNEL_BINARY.program_entry() as usize);

        let mut next_v_addr = kernel_virt_max.next_multiple_of(PAGE_SIZE);
        let mut next_v_addr = |p_addr: PAddr, page_count: usize, flags: Entry| {
            let v_addr = next_v_addr;
            next_v_addr += page_count * PAGE_SIZE;

            for i in 0..page_count {
                let offset = i * PAGE_SIZE;
                map_phys_to_virt(
                    PAddr::new(p_addr.addr() + offset),
                    VAddr::new(v_addr + offset),
                    flags
                );
            }

            VAddr::new(v_addr)
        };

        let stack_ptr = boot::allocate_pages(AllocateType::AnyPages, ELPYTI_KERNEL_STACK, KERNEL_STACK_PAGES).unwrap().as_ptr();
        let stack = next_v_addr(PAddr::new(stack_ptr.expose_provenance()), KERNEL_STACK_PAGES, Entry::WRITABLE);
        kernel_stack_base = VAddr::new(stack.addr() + KERNEL_STACK_PAGES * PAGE_SIZE);

        let pml4_ptr = boot::allocate_pages(AllocateType::AnyPages, ELPYTI_PAGE_TABLE, 1).unwrap().as_ptr();
        pml4_phys = PAddr::new(pml4_ptr.expose_provenance());
        let pml4_virt = next_v_addr(pml4_phys, 1, Entry::WRITABLE);

        let boot_info_page_len = size_of::<BootInfo>().div_ceil(PAGE_SIZE);
        boot_info_ptr = boot::allocate_pages(AllocateType::AnyPages, ELPYTI_BOOT_INFO, boot_info_page_len).unwrap().as_ptr().cast();
        unsafe {
            boot_info_ptr.write(BootInfo {
                graphics_info,
                pml4_table: pml4_virt.ptr_mut(),

                // Initialized after exiting UEFI boot services
                memory_regions_base: [MaybeUninit::uninit(); _],
                memory_regions_size: 0,
            });
        }
        boot_info = next_v_addr(PAddr::new(boot_info_ptr.expose_provenance()), boot_info_page_len, Entry::empty());

        let switcher_addr = (switch_to_kernel as *const ()).expose_provenance();
        assert_eq!(switcher_addr % PAGE_SIZE, 0, "`switch_to_kernel` must be page-aligned");
        map_phys_to_virt(PAddr::new(switcher_addr), VAddr::new(switcher_addr), Entry::empty());

        unsafe {
            pml4_ptr.cast::<Pml4Table>().write(pml4);
        }
    }

    unsafe {
        memory_map = boot::exit_boot_services(Some(boot::MemoryType::LOADER_DATA));
        
        let mut size = 0;
        for entry in memory_map.entries() {
            if matches!(entry.ty,
                MemoryType::LOADER_CODE | MemoryType::LOADER_DATA |
                MemoryType::BOOT_SERVICES_CODE | MemoryType::BOOT_SERVICES_DATA |
                MemoryType::CONVENTIONAL
            ) {
                (&raw mut (*boot_info_ptr).memory_regions_base[size]).cast::<MemoryRegion>().write(MemoryRegion {
                    base: PAddr::new(entry.phys_start as usize),
                    pages: entry.page_count as usize,
                });
                size += 1;
            }
        }

        (&raw mut (*boot_info_ptr).memory_regions_size).write(size);
    }

    UefiInfo {
        memory_map,
        pml4_phys,

        boot_info,
        kernel_stack_base,
        kernel_entry,
    }
}

#[rustc_align(4096)]
#[unsafe(naked)]
unsafe extern "sysv64" fn switch_to_kernel(
    pml4_phys: usize,
    boot_info: usize,
    stack_base: usize,
    kernel_entry: usize,
) -> ! {
    naked_asm!(
        "mov cr3, rdi",
        "lea rsp, [rdx - 8]",
        "mov rdi, rsi",
        "jmp rcx",
    )
}

#[entry]
fn entry() -> Status {
    let UefiInfo { memory_map, pml4_phys, boot_info, kernel_stack_base, kernel_entry } = setup_uefi_and_exit();
    unsafe {
        switch_to_kernel(pml4_phys.addr(), boot_info.addr(), kernel_stack_base.addr(), kernel_entry.addr())
    }
}
