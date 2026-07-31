ENTRY(_start)

PHDRS
{
    text    PT_LOAD    FLAGS(5);
    rodata  PT_LOAD    FLAGS(4);
    data    PT_LOAD    FLAGS(6);
    dynamic PT_DYNAMIC FLAGS(4);
}

SECTIONS
{
    __kernel_start = .;
    . = 0;

    .text : ALIGN(4K)
    {
        __text_start = .;
        *(.text.boot)
        *(.text .text.*)
        __text_end = .;
    } :text

    .rodata : ALIGN(4K)
    {
        __rodata_start = .;
        *(.rodata .rodata.*)
        __rodata_end = .;
    } :rodata

    .data : ALIGN(4K)
    {
        __data_start = .;
        *(.data .data.*)
        __data_end = .;
    } :data

    .bss (NOLOAD) : ALIGN(4K)
    {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    } :data

    .dynamic : ALIGN(8)
    {
        *(.dynamic)
    } :data :dynamic

    __kernel_end = .;
}