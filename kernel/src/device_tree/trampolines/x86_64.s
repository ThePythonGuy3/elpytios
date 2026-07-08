.section .ap_trampoline, "ax"

.global __ap_trampoline_start
.global __ap_trampoline_end

.code16
__ap_trampoline_start:
ap_entry_16:
    jmp 1f

    # Rust's inline assembler really doesn't like immediates for some reason
    .gdt_start_lookup:          .word .gdt_start - __ap_trampoline_start
    .gdt_desc_lookup:           .word .gdt_desc - __ap_trampoline_start
    .jmp_buf_lookup:            .word .jmp_buf - __ap_trampoline_start
    .ap_entry_32_lookup:        .word ap_entry_32 - __ap_trampoline_start

    .set GDT_START_LOOKUP,      .gdt_start_lookup - __ap_trampoline_start
    .set GDT_DESC_LOOKUP,       .gdt_desc_lookup - __ap_trampoline_start
    .set JMP_BUF_LOOKUP,        .jmp_buf_lookup - __ap_trampoline_start
    .set AP_ENTRY_32_LOOKUP,    .ap_entry_32_lookup - __ap_trampoline_start

    # Clear interrupts and zero out segments
1:  cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax

    # Get the allocated address of `__ap_trampoline_start` from `cs`
    xor bx, bx
    mov bx, cs
    shl bx, 4

    # Set `.gdt_desc + 2` to `.gdt_start` relative to `bx`
    mov si, bx
    add si, [bx + GDT_START_LOOKUP]
    mov di, [bx + GDT_DESC_LOOKUP]
    mov [bx + di + 2], si
    lgdt [bx + di]

    # Set `.jmp_buf + 2` to `ap_entry_32` relative to `bx`
    mov si, bx
    add si, [bx + AP_ENTRY_32_LOOKUP]
    mov di, [bx + JMP_BUF_LOOKUP]
    mov [bx + di + 2], si

    # Enable protected mode
    mov ecx, cr0
    or ecx, 0x00000001
    mov cr0, ecx

    # Raw bytes for a far jump; necessary since the address is dynamically written
.jmp_buf:
    .byte 0x66, 0xea
    .long 0x00000000
    .word 0x0008
.align 4
.gdt_start:
    .quad 0x0000000000000000
    .quad 0x00cf9a000000ffff
    .quad 0x00cf92000000ffff
.gdt_end:
.gdt_desc:
    .word .gdt_end - .gdt_start - 1
    .long 0x00000000

.code32
ap_entry_32:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax

    xor ax, ax
    mov fs, ax
    mov gs, ax

2:  jmp 2b

__ap_trampoline_end: