use std::{env::var_os, path::PathBuf};

fn main() {
    let mut manifest = PathBuf::from(var_os("CARGO_MANIFEST_DIR").expect("`CARGO_MANIFEST_DIR` not set"));
    manifest.push("src/linker.ld");

    println!("cargo:rustc-link-arg=-T{}", manifest.display());
}
