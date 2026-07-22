use crate::{arch::x86_64::ExtendedRegisterBuffer, interrupt::x86_64::InterruptFrame};

#[repr(C)]
pub struct Task {
    frame: InterruptFrame,
    registers: ExtendedRegisterBuffer,
}
