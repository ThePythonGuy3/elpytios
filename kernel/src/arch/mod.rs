cfg_select! {
    target_arch = "x86_64" => {
        pub mod x86_64;
    }
    _ => {
        compile_error!("Unsupported architecture");
    }
}
