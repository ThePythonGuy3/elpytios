set confirm off

target remote :1234
layout regs

file ./target/x86_64-unknown-none/bootloader_debug/elpytios-kernel -o 0xffff800000000000
add-symbol-file ./target/x86_64-unknown-none/bootloader_debug/elpytios-kernel -o 0xffff800000000000