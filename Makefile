.PHONY: run

all: src/main.rs
	cargo build --release
	cp ./target/x86_64-unknown-uefi/release/kernel.efi ./esp/EFI/BOOT/BOOTX64.EFI

run:
	qemu-system-x86_64 -accel whpx -drive if=pflash,format=raw,readonly=on,file=OVMF/OVMF_CODE.4m.fd -drive if=pflash,format=raw,readonly=on,file=OVMF/OVMF_VARS.4m.fd -drive format=raw,file=fat:rw:esp -drive format=qcow2,file=disk.qcow2 -machine q35 -m 4830196K
