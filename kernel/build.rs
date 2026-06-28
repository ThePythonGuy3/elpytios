use std::{env::var_os, path::PathBuf};

fn main() {
    let mut linker = PathBuf::from(var_os("CARGO_MANIFEST_DIR").expect("`CARGO_MANIFEST_DIR` not set"));
    linker.push("src/linker.ld");

    println!("cargo:rustc-link-arg=-T{}", linker.display());
    println!("cargo::rerun-if-changed={}", linker.display());
}
