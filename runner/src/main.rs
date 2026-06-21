use std::{env, fs, io, path::PathBuf, process::Command};

fn main() -> io::Result<()> {
    if !Command::new("cargo")
        .args(["build", "--release", "-p", "elpytios-bootloader"])
        .status()?
        .success()
    {
        Err(io::Error::new(io::ErrorKind::Other, "running `cargo build` failed"))?
    }

    let root = env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from).unwrap_or_default();
    let boot = root.join("esp/EFI/BOOT");

    fs::create_dir_all(&boot)?;
    fs::copy(
        //TODO adjust this when `elpytios-bootloader` gets its own crate
        root.join("../target/x86_64-unknown-uefi/release/elpytios-bootloader.efi"),
        boot.join("BOOTX64.efi"),
    )?;

    let file = root.join("disk.qcow2");
    if !file.exists()
        && !Command::new("qemu-img")
            .current_dir(&root)
            .args(["create", "-f", "qcow2"])
            .arg(&file)
            .arg("10G")
            .status()?
            .success()
    {
        Err(io::Error::new(io::ErrorKind::Other, "running `qemu-img` failed"))?
    }

    if !Command::new(format!("qemu-system-{}", env::consts::ARCH))
        .current_dir(&root)
        .args([
            "-accel",
            match env::consts::OS {
                "windows" => "whpx",
                "linux" => "kvm",
                _ => "tcg",
            },
            "-drive",
            "if=pflash,format=raw,readonly=on,file=OVMF/OVMF_CODE.4m.fd",
            "-drive",
            "if=pflash,format=raw,readonly=on,file=OVMF/OVMF_VARS.4m.fd",
            "-drive",
            "format=raw,file=fat:rw:esp",
            "-drive",
            "format=qcow2,file=disk.qcow2",
            "-machine",
            "q35",
            "-m",
            "4830196K",
            "-device",
            "virtio-vga",
            "-vga",
            "virtio"
        ])
        .status()?
        .success()
    {
        Err(io::Error::new(io::ErrorKind::Other, "running `qemu-system-*` failed"))?
    }

    Ok(())
}
