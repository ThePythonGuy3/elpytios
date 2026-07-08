.section .ap_trampoline, "a"

.global __ap_trampoline_start
.global __ap_pml4_phys
.global __ap_stack
.global __ap_kernel_entry
.global __ap_trampoline_end

__ap_trampoline_start:
.code16
ap_entry_16:
    # Clear interrupts and zero out segments
    cli
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

    # Set `.jmp_32 + 2` to `ap_entry_32` relative to `bx`
    mov si, bx
    add si, [bx + AP_ENTRY_32_LOOKUP]
    mov di, [bx + JMP_32_LOOKUP]
    mov [bx + di + 2], si

    # Enable protected mode
    mov ecx, cr0
    or ecx, 0x00000001
    mov cr0, ecx

    # Raw bytes for a far jump; necessary since the address is dynamically written
.jmp_32:
    .byte 0x66, 0xea
    .long 0x00000000
    .word 0x0008
.align 4
.gdt_start:
    .quad 0x0000000000000000 # Null
    .quad 0x00cf9a000000ffff # Kernel code 32-bit
    .quad 0x00cf92000000ffff # Kernel data
    .quad 0x00af9a000000ffff # Kernel code 64-bit
.gdt_end:
.gdt_desc:
    .word .gdt_end - .gdt_start - 1
    .long 0x00000000

    .gdt_start_lookup:          .word .gdt_start - __ap_trampoline_start
    .gdt_desc_lookup:           .word .gdt_desc - __ap_trampoline_start
    .jmp_32_lookup:             .word .jmp_32 - __ap_trampoline_start
    .ap_entry_32_lookup:        .word ap_entry_32 - __ap_trampoline_start
    .equ GDT_START_LOOKUP,      .gdt_start_lookup - __ap_trampoline_start
    .equ GDT_DESC_LOOKUP,       .gdt_desc_lookup - __ap_trampoline_start
    .equ JMP_32_LOOKUP,         .jmp_32_lookup - __ap_trampoline_start
    .equ AP_ENTRY_32_LOOKUP,    .ap_entry_32_lookup - __ap_trampoline_start

.code32
ap_entry_32:
    # Reinitialize segments and zero-extend base address
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    movzx ebx, bx

    # Zero out unused segments
    xor ax, ax
    mov fs, ax
    mov gs, ax

    # Set physical page table
    mov eax, [ebx + PML4_PHYS]
    mov cr3, eax

    # Enable Page Address Extension
    mov eax, cr4
    or eax, 1 << 5
    mov cr4, eax

    # Enable 64-bit long mode
    mov ecx, 0xc0000080 # `IA32_EFER`
    rdmsr
    # # `IA32_EFER`
    or eax, 1 << 8
    wrmsr

    # Set `.jmp_64 + 2` to `ap_entry_64` relative to `ebx`
    mov esi, ebx
    add esi, [ebx + AP_ENTRY_64_LOOKUP]
    mov edi, [ebx + JMP_64_LOOKUP]
    mov [ebx + edi + 1], esi

    # Enable virtual paging
    mov eax, cr0
    or eax, 1 << 31
    mov cr0, eax

    # Raw bytes for a far jump; necessary since the address is dynamically written
.jmp_64:
    .byte 0xea
    .long 0x00000000
    .word 0x0018

    __ap_pml4_phys:             .long 0x00000000
    .jmp_64_lookup:             .long .jmp_64 - __ap_trampoline_start
    .ap_entry_64_lookup:        .long ap_entry_64 - __ap_trampoline_start
    .equ PML4_PHYS,             __ap_pml4_phys - __ap_trampoline_start
    .equ JMP_64_LOOKUP,         .jmp_64_lookup - __ap_trampoline_start
    .equ AP_ENTRY_64_LOOKUP,    .ap_entry_64_lookup - __ap_trampoline_start

.code64
ap_entry_64:
1:  jmp 1b

    __ap_stack:         .quad 0x0000000000000000
    __ap_kernel_entry:  .quad 0x0000000000000000

__ap_trampoline_end: