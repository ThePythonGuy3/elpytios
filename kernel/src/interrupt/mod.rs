cfg_select! {
    target_arch = "x86_64" => {
        pub mod x86_64;
        pub use x86_64::init_interrupts;
    }
    _ => {
        compile_error!("Unsupported architecture");
    }
}

#[inline]
unsafe fn page_fault(
    ptr: *mut (),
    missing_or_protected: bool,
    caused_by_write: bool,
    triggered_by_user: bool,
    overwritten_reserved_bits: bool,
    instruction_fetch_violation: bool,
) {
    panic!(
        "Page fault at address {ptr:p}\n\
         Missing/access\t: {missing_or_protected}\n\
         Caused by write\t: {caused_by_write}\n\
         From userland\t: {triggered_by_user}\n\
         Reserved bits\t: {overwritten_reserved_bits}\n\
         Inst fetch\t: {instruction_fetch_violation}"
    )
}

#[inline]
unsafe fn mem_map(file: usize, offset: usize, page_count: usize, flags: usize) -> *mut u8 {
    panic!(
        "SYSCALL mem_map\n\
         File\t: {file}\n\
         Offset\t: {offset}\n\
         Count\t: {page_count}\n\
         Flags\t: {flags}"
    )
}
