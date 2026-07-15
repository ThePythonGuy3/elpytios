use alloc::boxed::Box;

use crate::{arch::x86_64::ExtendedRegisterBuffer, interrupt::x86_64::InterruptFrame};

pub struct Task {
    frame: InterruptFrame,
    registers: Box<ExtendedRegisterBuffer>,
}
