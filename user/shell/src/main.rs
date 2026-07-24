fn main() {
    unsafe {
        elpytios_abi::Syscall::write(3, 1, 4);
    }
    loop {}
}
