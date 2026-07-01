pub use imp::init_interrupts;
pub(crate) use imp::*;

cfg_select! {
    target_arch = "x86_64" => {
        mod x86_64;
        use x86_64 as imp;
    }
    _ => {
        compile_error!("Unsupported architecture");
    }
}
