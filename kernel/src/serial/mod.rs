cfg_select! {
    target_arch = "x86_64" => {
        mod uart;
        pub use uart::*;
    }
    _ => {
        compile_error!("Unsupported architecture");
    }
}
