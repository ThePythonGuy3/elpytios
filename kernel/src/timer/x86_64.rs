use core::{
    arch::x86_64::{__cpuid_count, _rdtsc},
    hint::spin_loop,
    time::Duration,
};

use crate::arch::x86_64::{Msr, wrmsr};

pub enum Timer {
    Tsc { clock_hz: u64 },
}

impl Timer {
    pub(super) fn new() -> Self {
        if let Some(clock_hz) = tsc_frequency() {
            Self::Tsc { clock_hz }
        } else {
            unimplemented!("Timer implementation fallback (FADT, HPET, legacy PIT)")
        }
    }

    pub(super) fn busy_wait(&self, duration: Duration) {
        match *self {
            Self::Tsc { clock_hz } => {
                let ticks = Self::to_ticks(duration, clock_hz);
                let start = unsafe { _rdtsc() };

                while unsafe { _rdtsc() }.wrapping_sub(start) < ticks {
                    spin_loop();
                }
            }
        }
    }

    pub(super) fn schedule(&self, duration: Duration) {
        match *self {
            Self::Tsc { clock_hz } => {
                let ticks = Self::to_ticks(duration, clock_hz);
                unsafe {
                    wrmsr(Msr::Ia32TscDeadline, _rdtsc().wrapping_add(ticks));
                }
            }
        }
    }

    fn to_ticks(duration: Duration, clock_hz: u64) -> u64 {
        let secs = duration.as_secs() as u128;
        let nanos = duration.subsec_nanos() as u128;
        let hz = clock_hz as u128;

        let ticks_secs = secs * hz;
        let ticks_nanos = (nanos * hz) / 1_000_000_000;

        u64::try_from(ticks_secs + ticks_nanos).unwrap_or(u64::MAX)
    }
}

fn tsc_frequency() -> Option<u64> {
    // TSC is only used if TSC-deadline mode is supported
    let leaf_deadline = __cpuid_count(0x1, 0x0);
    if leaf_deadline.ecx & (1 << 24) == 0 {
        return None
    }

    let max_leaf = __cpuid_count(0x0, 0x0).eax;

    // Core crystal clock
    if max_leaf >= 0x15 {
        let leaf = __cpuid_count(0x15, 0x0);
        let denom = leaf.eax;
        let numer = leaf.ebx;
        let hz = leaf.ecx;

        if hz != 0
            && denom != 0
            && numer != 0
            && let Ok(freq) = u64::try_from(hz as u128 * numer as u128 / denom as u128)
        {
            return Some(freq)
        }
    }

    // Processor frequency (fallback)
    if max_leaf >= 0x16 {
        let leaf = __cpuid_count(0x16, 0x0);
        let mhz = leaf.eax as u64;

        if mhz != 0
            && let Ok(freq) = u64::try_from(mhz as u128 * 1_000_000)
        {
            return Some(freq)
        }
    }

    // Hypervisor timing info (fallback, QEMU/KVM)
    if leaf_deadline.ecx & (1 << 31) != 0 {
        let leaf_hv = __cpuid_count(0x4000_0000, 0x0);
        if leaf_hv.eax >= 0x4000_0010 {
            let leaf_time = __cpuid_count(0x4000_0010, 0x0);
            let khz = leaf_time.eax;

            if khz != 0
                && let Ok(freq) = u64::try_from(khz as u128 * 1_000)
            {
                return Some(freq)
            }
        }
    }

    None
}
