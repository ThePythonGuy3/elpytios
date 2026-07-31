use core::arch::asm;

#[inline(always)]
pub unsafe fn outb(port: u16, value: u8) {
    unsafe {
        asm!(
            "outb %al, %dx",
            in("al") value,
            in("dx") port,

            options(att_syntax, nomem, nostack, preserves_flags)
        )
    }
}

#[inline(always)]
pub unsafe fn inb(port: u16) -> u8 {
    unsafe {
        let value: u8;
        asm!(
            "inb %dx, %al",
            in("dx") port,
            out("al") value,

            options(att_syntax, nomem, nostack, preserves_flags),
        );
        value
    }
}
