use core::{iter, ops::RangeInclusive};

use bitflags::bitflags;

use crate::device_tree::{
    SystemTable,
    acpi::{PackedPtr, sealed::TypedSystemTable},
};

#[derive(Clone, Copy)]
pub struct Madt<'root> {
    pub local_interrupt_control_addr: u32,
    pub flags: ApicFlags,
    payload: PackedPtr<'root>,
}

impl<'root> IntoIterator for Madt<'root> {
    type IntoIter = impl Iterator<Item = Pic<'root>> + 'root;
    type Item = <Self::IntoIter as Iterator>::Item;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        let mut payload = self.payload;
        iter::from_fn(move || {
            while payload.len() != 0 {
                const RESERVED_FOR_OSPM_START: u8 = *RESERVED_FOR_OSPM.start();
                const RESERVED_FOR_OSPM_END: u8 = *RESERVED_FOR_OSPM.end();
                const RESERVED_FOR_OEM_START: u8 = *RESERVED_FOR_OEM.start();
                const RESERVED_FOR_OEM_END: u8 = *RESERVED_FOR_OEM.end();

                unsafe {
                    let kind: u8 = payload.read();
                    let len: u8 = payload.read();

                    return Some(match kind {
                        PROCESSOR_LOCAL => Pic::ProcessorLocal(payload.read()),
                        IO => Pic::Io(payload.read()),
                        INTERRUPT_SOURCE_OVERRIDE => Pic::InterruptSourceOverride(payload.read()),
                        NMI_SOURCE => Pic::NmiSource(payload.read()),
                        LOCAL_NMI => Pic::LocalNmi(payload.read()),
                        LOCAL_ADDR_OVERRIDE => Pic::LocalAddrOverride(payload.read()),
                        IO_STREAMLINED => Pic::IoStreamlined(payload.read()),
                        PROCESSOR_LOCAL_STREAMLINED => {
                            Pic::ProcessorLocalStreamlined(payload.read(), payload.slice(len as usize - 2 - size_of::<ProcessorLocalStreamlined>()))
                        }
                        PLATFORM_INTERRUPT_SOURCES => Pic::PlatformInterruptSources(payload.read()),
                        PROCESSOR_LOCAL_X2 => Pic::ProcessLocalX2(payload.read()),
                        LOCAL_NMI_X2 => Pic::LocalNmiX2(payload.read()),
                        GIC_CPU_INTERFACE => Pic::GicCpuInterface(payload.read()),
                        GIC_DISTRIBUTOR => Pic::GicDistributor(payload.read()),
                        GIC_MSI_FRAME => Pic::GicMsiFrame(payload.read()),
                        GIC_REDISTRIBUTOR => Pic::GicRedistributor(payload.read()),
                        GIC_INTERRUPT_TRANSLATION_SERVICE => Pic::GicInterruptTranslationService(payload.read()),
                        RESERVED_FOR_OSPM_START..=RESERVED_FOR_OSPM_END | RESERVED_FOR_OEM_START..=RESERVED_FOR_OEM_END => continue,
                    })
                }
            }
            None
        })
    }
}

unsafe impl TypedSystemTable for Madt<'_> {
    const SIGNATURE: [u8; 4] = *b"APIC";
    type Out<'root> = Madt<'root>;

    #[inline]
    unsafe fn from_table<'root>(table: &SystemTable<'root>) -> Madt<'root> {
        let mut payload = PackedPtr::new(table.entries);
        unsafe {
            Madt {
                local_interrupt_control_addr: payload.read(),
                flags: payload.read(),
                payload,
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct ApicFlags(u32);
bitflags! {
    impl ApicFlags: u32 {
        const PCAT_COMPAT = 1 << 0;
    }
}

pub const PROCESSOR_LOCAL: u8 = 0x00;
pub const IO: u8 = 0x01;
pub const INTERRUPT_SOURCE_OVERRIDE: u8 = 0x02;
pub const NMI_SOURCE: u8 = 0x03;
pub const LOCAL_NMI: u8 = 0x04;
pub const LOCAL_ADDR_OVERRIDE: u8 = 0x05;
pub const IO_STREAMLINED: u8 = 0x06;
pub const PROCESSOR_LOCAL_STREAMLINED: u8 = 0x07;
pub const PLATFORM_INTERRUPT_SOURCES: u8 = 0x08;
pub const PROCESSOR_LOCAL_X2: u8 = 0x09;
pub const LOCAL_NMI_X2: u8 = 0x0a;
pub const GIC_CPU_INTERFACE: u8 = 0x0b;
pub const GIC_DISTRIBUTOR: u8 = 0x0c;
pub const GIC_MSI_FRAME: u8 = 0x0d;
pub const GIC_REDISTRIBUTOR: u8 = 0x0e;
pub const GIC_INTERRUPT_TRANSLATION_SERVICE: u8 = 0x0f;
pub const RESERVED_FOR_OSPM: RangeInclusive<u8> = 0x10..=0x7f;
pub const RESERVED_FOR_OEM: RangeInclusive<u8> = 0x80..=0xff;

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum Pic<'root> {
    ProcessorLocal(ProcessorLocal) = PROCESSOR_LOCAL,
    Io(Io) = IO,
    InterruptSourceOverride(InterruptSourceOverride) = INTERRUPT_SOURCE_OVERRIDE,
    NmiSource(NmiSource) = NMI_SOURCE,
    LocalNmi(LocalNmi) = LOCAL_NMI,
    LocalAddrOverride(LocalAddrOverride) = LOCAL_ADDR_OVERRIDE,
    IoStreamlined(IoStreamlined) = IO_STREAMLINED,
    ProcessorLocalStreamlined(
        ProcessorLocalStreamlined,
        &'root [u8], // `acpi_processor_uid_string`
    ) = PROCESSOR_LOCAL_STREAMLINED,
    PlatformInterruptSources(PlatformInterruptSources) = PLATFORM_INTERRUPT_SOURCES,
    ProcessLocalX2(ProcessLocalX2) = PROCESSOR_LOCAL_X2,
    LocalNmiX2(LocalNmiX2) = LOCAL_NMI_X2,
    GicCpuInterface(GicCpuInterface) = GIC_CPU_INTERFACE,
    GicDistributor(GicDistributor) = GIC_DISTRIBUTOR,
    GicMsiFrame(GicMsiFrame) = GIC_MSI_FRAME,
    GicRedistributor(GicRedistributor) = GIC_REDISTRIBUTOR,
    GicInterruptTranslationService(GicInterruptTranslationService) = GIC_INTERRUPT_TRANSLATION_SERVICE,
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct ProcessorLocal {
    pub acpi_processor_uid: u8,
    pub apic_id: u8,
    pub apic_flags: LocalApicFlags,
}

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct LocalApicFlags(u32);
bitflags! {
    impl LocalApicFlags: u32 {
        const ENABLED        = 1 << 0;
        const ONLINE_CAPABLE = 1 << 1;
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct Io {
    pub io_apic_id: u8,
    pub reserved: [u8; 1],
    pub io_apic_addr: u32,
    pub global_system_interrupt_base: u32,
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct InterruptSourceOverride {
    pub bus: u8,
    pub source: u8,
    pub global_system_interrupt: u32,
    pub flags: MpsIntiFlags,
}

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct MpsIntiFlags(u16);
bitflags! {
    impl MpsIntiFlags: u16 {
        const ACTIVE_HIGH     = 0b01;
        const ACTIVE_LOW      = 0b11;
        const POLARITY        = 0b11;

        const EDGE_TRIGGERED  = 0b01 << 2;
        const LEVEL_TRIGGERED = 0b11 << 2;
        const TRIGGER_MODE    = 0b11 << 2;
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct NmiSource {
    pub flags: MpsIntiFlags,
    pub global_system_interrupt: u32,
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct LocalNmi {
    pub acpi_processor_uid: u8,
    pub flags: MpsIntiFlags,
    pub local_apic_lint_num: u8,
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct LocalAddrOverride {
    pub reserved: [u8; 2],
    pub local_apic_addr: u64,
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct IoStreamlined {
    pub io_apic_id: u8,
    pub reserved: [u8; 1],
    pub global_system_interrupt_base: u32,
    pub io_sapic_addr: u64,
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct ProcessorLocalStreamlined {
    pub acpi_processor_id: u8,
    pub local_sapic_id: u8,
    pub local_sapic_eid: u8,
    pub reserved: [u8; 3],
    pub flags: LocalApicFlags,
    pub acpi_processor_uid_value: u32,
    // Note: `acpi_processor_uid_string: [u8; leftover]`
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct PlatformInterruptSources {
    pub flags: MpsIntiFlags,
    pub interrupt_type: u8,
    pub processor_id: u8,
    pub processor_eid: u8,
    pub io_sapic_vector: u8,
    pub global_system_interrupt: u32,
    pub platform_interrupt_source_flags: PlatformInterruptSourceFlags,
}

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct PlatformInterruptSourceFlags(u32);
bitflags! {
    impl PlatformInterruptSourceFlags: u32 {
        const CPEI_PROCESSOR_OVERRIDE = 1 << 0;
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct ProcessLocalX2 {
    pub reserved: [u8; 2],
    pub x2apic_id: u32,
    pub flags: LocalApicFlags,
    pub acpi_processor_uid: u32,
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct LocalNmiX2 {
    pub flags: MpsIntiFlags,
    pub acpi_processor_uid: u32,
    pub local_x2apic_lint_num: u8,
    pub reserved: [u8; 3],
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct GicCpuInterface {
    pub reserved: [u8; 2],
    pub cpu_interface_num: u32,
    pub acpi_processor_uid: u32,
    pub flags: GicCpuInterfaceFlags,
    pub parking_protocol_version: u32,
    pub performance_interrupt_gsiv: u32,
    pub parked_addr: u64,
    pub physical_base_addr: u64,
    pub gicv: u64,
    pub gich: u64,
    pub vgic_maintenance_interrupt: u32,
    pub gicr_base_addr: u64,
    pub mpidr: u64,
    pub processor_power_efficiency_class: u8,
    pub reserved2: [u8; 1],
    pub spe_overflow_interrupt: u16,
}

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct GicCpuInterfaceFlags(u32);
bitflags! {
    impl GicCpuInterfaceFlags: u32 {
        const ENABLED                         = 1 << 0;
        const PERFORMANCE_INTERRUPT_MODE      = 1 << 1;
        const VGIC_MAINTENANCE_INTERRUPT_MODE = 1 << 2;
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct GicDistributor {
    pub reserved: [u8; 2],
    pub gic_id: u32,
    pub physical_base_addr: u64,
    pub system_vector_base: u32,
    pub gic_version: u8,
    pub reserved2: [u8; 3],
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct GicMsiFrame {
    pub reserved: [u8; 2],
    pub gic_msi_frame_id: u32,
    pub physical_base_addr: u64,
    pub flags: GicMsiFrameFlags,
    pub spi_count: u16,
    pub spi_base: u16,
}

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct GicMsiFrameFlags(u32);
bitflags! {
    impl GicMsiFrameFlags: u32 {
        const SPI_COUNT_BASE_SELECT = 1 << 0;
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct GicRedistributor {
    pub reserved: [u8; 2],
    pub discovery_range_base_addr: u64,
    pub discovery_range_len: u32,
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct GicInterruptTranslationService {
    pub reserved: [u8; 2],
    pub gic_its_id: u32,
    pub physical_base_addr: u64,
    pub reserved2: [u8; 4],
}
