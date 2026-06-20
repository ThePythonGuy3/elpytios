use uefi::{boot::MemoryType, mem::memory_map::{MemoryMap, MemoryMapOwned}};

#[derive(Clone, Copy)]
pub struct PhysicalPageMetadata {
    pub free:     bool, // Is the page free
    pub locked:   bool, // Is the page not for OS use
    pub loader:   bool, // Does the page contain UEFI loader ranges
    pub reserved: bool
}

impl PhysicalPageMetadata {
    fn get_value(&self) -> u8 {
        ((self.free     as u8) << 3) |
        ((self.locked   as u8) << 2) |
        ((self.loader   as u8) << 1) |
        ((self.reserved as u8))
    }

    fn from_value(value: u8) -> Self {
        Self {
            free:     (value >> 3) & 1 != 0,
            locked:   (value >> 2) & 1 != 0,
            loader:   (value >> 1) & 1 != 0,
            reserved: (value)      & 1 != 0
        }
    }
}

pub struct PhysicalPageAllocator {
    base: *mut u8
}

impl PhysicalPageAllocator {
    const PAGE_SIZE: u64 = 4096;
    const UNKNOWN_PAGE_METADATA: PhysicalPageMetadata = PhysicalPageMetadata {
        free:     false,
        locked:   true,
        loader:   false,
        reserved: false
    };

    unsafe fn write_entry(&self, entry: usize, metadata: &PhysicalPageMetadata) {
        unsafe {
            let true_entry = self.base.add(entry >> 1);

            let value: u8 = metadata.get_value();

            let current_value = true_entry.read_volatile();

            let value_to_write: u8;
            if entry & 1 == 0 {
                value_to_write = (current_value & 0x0F) | (value << 4);
            } else {
                value_to_write = (current_value & 0xF0) | value;
            }

            true_entry.write_volatile(value_to_write);
        }
    }

    unsafe fn write_entries(&self, entry: usize, metadata_high: &PhysicalPageMetadata, metadata_low: &PhysicalPageMetadata) {
        unsafe {
            let true_entry = self.base.add(entry >> 1);

            let value = metadata_high.get_value() << 4 | metadata_low.get_value();

            true_entry.write_volatile(value);
        }
    }

    unsafe fn read_entry(&self, entry: usize) -> PhysicalPageMetadata {
        unsafe {
            let (first, second) = self.read_entries(entry);

            if entry & 1 == 0 { first } else { second }
        }
    }

    unsafe fn read_entries(&self, entry: usize) -> (PhysicalPageMetadata, PhysicalPageMetadata) {
        unsafe {
            let value = self.base.add(entry >> 1).read_volatile();

            (
                PhysicalPageMetadata::from_value(value >> 4),
                PhysicalPageMetadata::from_value(value)
            )
        }
    }

    pub unsafe fn new(memory_map: &MemoryMapOwned) -> Option<Self> {
        let mut base: u64 = 0;
        for i in &mut memory_map.entries() {
            match i.ty {
                MemoryType::BOOT_SERVICES_CODE |
                MemoryType::BOOT_SERVICES_DATA |
                MemoryType::CONVENTIONAL       |
                MemoryType::PERSISTENT_MEMORY if i.phys_start != 0 => {
                    base = i.phys_start;
                    break;
                },
                _ => {}
            }
        }

        if base == 0 {
            return None;
        }

        let allocator = PhysicalPageAllocator {
            base: base as *mut u8
        };

        let mut previous_addr: u64 = 0;
        let mut entry:         u64 = 0;

        let mut entry_type: [&PhysicalPageMetadata; 2] = [&Self::UNKNOWN_PAGE_METADATA, &Self::UNKNOWN_PAGE_METADATA];

        for i in &mut memory_map.entries() {
            let addr = i.phys_start;
            let max_addr = addr + i.page_count * Self::PAGE_SIZE;

            while previous_addr < max_addr {
                if previous_addr < addr {
                    entry_type[(entry % 2) as usize] = &Self::UNKNOWN_PAGE_METADATA;
                }

                previous_addr += Self::PAGE_SIZE;
                entry += 1;
            }
        }

        Some(allocator)
    }
}
