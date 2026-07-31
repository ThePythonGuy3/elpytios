use core::{fmt, hint::spin_loop};

use crate::arch::x86_64::{inb, outb};

#[derive(Debug, Clone, Copy)]
#[repr(u16)]
pub enum Com {
    Com1 = 0x3f8,
    Com2 = 0x2f8,
    Com3 = 0x3e8,
    Com4 = 0x2e8,
}

pub unsafe fn serial_init(port: Com) {
    let port = port as u16;
    unsafe {
        // Disable interrupts
        outb(port + 1, 0x00);

        // Enable DLAB
        outb(port + 3, 0x80);

        // Baud divisor = 3 (38400 baud assuming 115200 clock)
        outb(port + 0, 0x03);
        outb(port + 1, 0x00);

        // 8 bits, no parity, one stop bit
        outb(port + 3, 0x03);

        // Enable FIFO, clear them, 14-byte threshold
        outb(port + 2, 0xC7);

        // IRQs disabled, RTS/DSR set
        outb(port + 4, 0x03);
    }
}

#[inline]
fn tx_ready(port: Com) -> bool {
    unsafe { inb(port as u16 + 5) & 0x20 != 0 }
}

pub fn serial_write_byte(port: Com, byte: u8) {
    while !tx_ready(port) {
        spin_loop();
    }

    unsafe {
        outb(port as u16, byte);
    }
}

pub fn serial_write(port: Com, s: &str) {
    for b in s.bytes() {
        if b == b'\n' {
            serial_write_byte(port, b'\r');
        }

        serial_write_byte(port, b);
    }
}

pub struct Serial(pub Com);
impl fmt::Write for Serial {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        serial_write(self.0, s);
        Ok(())
    }
}
