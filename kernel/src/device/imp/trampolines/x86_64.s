.section .ap_trampoline, "aw"

.global __ap_trampoline_start
.global __ap_trampoline_size

__ap_trampoline_start:
.code16
ap_entry_16:
    # Clear interrupts and zero out segments
    cli
    xorl %eax, %eax
    mov %ax, %ds
    mov %ax, %es
    mov %ax, %ss

    # Get the allocated address of `__ap_trampoline_start` from `cs`
    xorl %ebx, %ebx
    movw %cs, %bx
    shll $4, %ebx

    # Set `.gdt_desc + 2` to `.gdt_start` relative to `bx`
    leaw (.gdt_start - __ap_trampoline_start)(%bx), %ax
    movw %ax, (.gdt_desc - __ap_trampoline_start + 2)(%bx)
    lgdt (.gdt_desc - __ap_trampoline_start)(%bx)

    # Set `.jmp_32 + 2` to `ap_entry_32` relative to `bx`
    leaw (ap_entry_32 - __ap_trampoline_start)(%bx), %ax
    movw %ax, (.jmp_32 - __ap_trampoline_start + 2)(%bx)

    # Enable protected mode
    movl %cr0, %eax
    orl $0x00000001, %eax
    movl %eax, %cr0

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

.code32
ap_entry_32:
    # Reinitialize segments and zero-extend base address
    movw $0x10, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss

    # Zero out unused segments
    xorw %ax, %ax
    movw %ax, %fs
    movw %ax, %gs

    # Set physical page table
    movl (__ap_cr3 - __ap_trampoline_start)(%ebx), %eax
    movl %eax, %cr3
    # Copy CR4 (which importantly includes Page Address Extension)
    movl (__ap_cr4 - __ap_trampoline_start)(%ebx), %eax
    movl %eax, %cr4

    # Enable 64-bit long mode
    movl $0xc0000080, %ecx # `IA32_EFER`
    rdmsr
    orl $(1 << 8), %eax
    orl $(1 << 11), %eax
    wrmsr

    # Set `.jmp_64 + 1` to `ap_entry_64` relative to `ebx`
    movl $(ap_entry_64 - __ap_trampoline_start), %eax
    addl %ebx, %eax
    movl %eax, (.jmp_64 - __ap_trampoline_start + 1)(%ebx)

    # Enable virtual paging
    movl %cr0, %eax
    orl $(1 << 31), %eax
    movl %eax, %cr0

    # Raw bytes for a far jump; necessary since the address is dynamically written
.jmp_64:
    .byte 0xea
    .long 0x00000000
    .word 0x0018

.code64
ap_entry_64:
    # Setup stack pointer to the stack given by BSP
    movq __ap_stack(%rip), %rsp
    andq $-16, %rsp

    # Call an `extern "sysv64"` function given by the kernel
    movq __ap_kernel_arg0(%rip), %rdi
    movq __ap_kernel_arg1(%rip), %rsi
    movq __ap_kernel_arg2(%rip), %rdx
    jmpq *__ap_kernel_entry(%rip)

.global __ap_cr3
.global __ap_cr4
.global __ap_stack
.global __ap_kernel_entry
.global __ap_kernel_arg0
.global __ap_kernel_arg1
.global __ap_kernel_arg2

.align 8
__ap_cr3:           .long 0x00000000
__ap_cr4:           .long 0x00000000
__ap_stack:         .quad 0x0000000000000000
__ap_kernel_entry:  .quad 0x0000000000000000
__ap_kernel_arg0:   .quad 0x0000000000000000
__ap_kernel_arg1:   .quad 0x0000000000000000
__ap_kernel_arg2:   .quad 0x0000000000000000

__ap_trampoline_end:
__ap_trampoline_size:   .quad __ap_trampoline_end - __ap_trampoline_start