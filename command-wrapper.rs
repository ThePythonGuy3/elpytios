use std::{
    env::{args_os, current_exe},
    fs,
    process::Command,
};

fn main() -> ! {
    let file = fs::canonicalize(current_exe().expect("Couldn't get `current_exe()`"))
        .expect("Couldn't canonicalize exe path")
        .with_extension("");
    let mut command = Command::new(file);
    for arg in args_os().skip(1) {
        command.arg(arg);
    }

    let status = command.status().expect("Couldn't spawn command process");
    std::process::exit(match status.code() {
        Some(code) => code,
        None => cfg_select! {
            unix => {
                {
                    use std::os::unix::process::ExitStatusExt;
                    if let Some(signal) = status.signal() {
                        128 + signal
                    } else {
                        -1
                    }
                }
            }
            _ => -1,
        },
    })
}
