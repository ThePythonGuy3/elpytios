#!/usr/bin/env python3

import argparse
import re
import os
import platform
import shutil
import subprocess
import sys

try:
    import tomlkit
    from tomlkit import TOMLDocument
    from tomlkit.items import String as TomlString
except ImportError:
    print("Please install `tomlkit`", file=sys.stderr)
    sys.exit(1)

from pathlib import Path

root = Path(__file__).resolve().parent

elpytios_abi_root = root / "abi"
elpytios_std_root = root / "std"
library_dst_root = elpytios_std_root / "rust-src"

sysroot = elpytios_std_root / "sysroot"
sysroot_libdir = sysroot / "lib" / "rustlib" / "x86_64-unknown-elpytios" / "lib"

libstd_dir = library_dst_root / "std"
std_dir_items = ["benches", "src", "tests", "build.rs", "Cargo.toml"]

std_fetcher_version = 0
rustc_version = subprocess.run(["rustc", "--version"], cwd=root, capture_output=True, text=True, check=True).stdout.strip()

def fetch_std(_args):
    # Copy `rust-src` component to `./std/rust-src`
    rustc_sys_root = subprocess.run(
        ["rustc", "--print", "sysroot"],
        cwd=root,
        capture_output=True,
        text=True,
        check=True
    )

    library_src_root = (
        Path(rustc_sys_root.stdout.strip())
        / "lib"
        / "rustlib"
        / "src"
        / "rust"
        / "library"
    )

    if not library_src_root.exists():
        raise RuntimeError("Missing `rust-src`, run `rustup component add rust-src`")

    clean_std(_args)
    shutil.copytree(library_src_root, library_dst_root)
    (library_dst_root / "Cargo.toml").unlink()

    shutil.rmtree(libstd_dir / "src" / "sys")

    class Package:
        path: str
        file: Path
        manifest: TOMLDocument

        def __init__(self, path: str, file: Path, manifest: TOMLDocument):
            self.path = path
            self.file = file
            self.manifest = manifest

    packages: dict[str, Package] = {}
    for manifest_file in library_dst_root.rglob("Cargo.toml"):
        manifest = tomlkit.parse(manifest_file.read_text(encoding="utf-8"))
        if (package := manifest.get("package")):
            if "edition" not in package:
                package["edition"] = "2024"

            manifest.pop("dev-dependencies", None)
            manifest.pop("profile", None)
            package.pop("resolver", None)

            packages[package["name"]] = Package(str(manifest_file.parent), manifest_file, manifest)

    # Copy `std` into a more visible folder for neatness purposes
    for item in std_dir_items:
        shutil.move(Path(packages["std"].path) / item, elpytios_std_root / item)
    (elpytios_std_root / "build.rs").write_text((elpytios_std_root / "build.rs").read_text(encoding="utf-8").replace(
        'if target_os == "linux"',
        'if target_os == "linux" || target_os == "elpytios"',
        1
    ), encoding="utf-8")

    packages["std"].path = str(elpytios_std_root)
    packages["std"].file = elpytios_std_root / "Cargo.toml"

    path_attr_re = re.compile(r'#\s*\[\s*path\s*=\s*"([^"]+)"\s*\]')
    include_re = re.compile(r'\b(include|include_str|include_bytes|concat)!\s*\(\s*"([^"]+)"\s*')
    def to_abs(current_path: Path, rel_path: str):
        abs_path = (current_path / rel_path).resolve()
        if abs_path.is_relative_to(libstd_dir):
            return rel_path
        else:
            return abs_path.as_posix() + "/" if abs_path.is_dir() else abs_path.as_posix()

    for file in (elpytios_std_root / "src").rglob("*.rs"):
        current_path = libstd_dir / "src" / file.parent.relative_to(elpytios_std_root / "src")
        file_rs = file.read_text(encoding="utf-8")
        file_rs = path_attr_re.sub(lambda m: f'#[path = "{to_abs(current_path, m.group(1))}"]', file_rs)
        file_rs = include_re.sub(lambda m: f'{m.group(1)}!("{to_abs(current_path, m.group(2))}"', file_rs)

        if file == elpytios_std_root / "src" / "lib.rs":
            file_rs = file_rs.replace("mod sys;", '#[path = "../sys-src/mod.rs"]\nmod sys;', 1)

        file.write_text(file_rs, encoding="utf-8")

    for package in packages.values():
        def visit_deps(dependencies):
            for name, spec in list(dependencies.items()):
                if (dep_package := packages.get(name)):
                    if isinstance(spec, TomlString):
                        dependencies[name] = { "path": dep_package.path }
                    else:
                        if name == "core" or name == "alloc" or name == "std":
                            spec.pop("package", None)

                        spec.pop("version", None)
                        spec["path"] = dep_package.path
                else:
                    # Non-vendored dependency means it's not used *at all* in `std`
                    del dependencies[name]

        if (deps := package.manifest.get("dependencies")):
            visit_deps(deps)
        if (target := package.manifest.get("target")):
            for _, target_spec in target.items():
                if (target_deps := target_spec.get("dependencies")):
                    visit_deps(target_deps)

        if (features := package.manifest.get("features")):
            for feat_name, feat_deps in list(features.items()):
                filtered = []
                for item in feat_deps:
                    if "/" in item:
                        dep = item.split("/", 1)[0]
                    elif "dep:" in item:
                        dep = item[4:]
                    else:
                        dep = item

                    if (dep != feat_name and dep in features) or dep in packages:
                        filtered.append(item)

                features[feat_name] = filtered

            # Forcibly override the defaults, since `rust-analyzer` is stupid
            if "rustc-dep-of-std" in features:
                features["default"] = ["rustc-dep-of-std"]
            elif "std" in features and (defaults := features.get("default")) and "std" in defaults:
                defaults.remove("std")

    for name in ["foldhash"]:
        packages[name].manifest["dependencies"]["core"] = { "path": packages["core"].path }

    for name in ["adler2", "cfg-if", "foldhash", "fortanix-sgx-abi", "hermit-abi", "memchr", "object", "panic_abort", "r-efi", "rustc-demangle", "vex-sdk", "wasip1"]:
        packages[name].manifest["dependencies"]["compiler_builtins"] = {
            "path": packages["compiler_builtins"].path,
            "features": ["compiler-builtins"],
        }

    packages["windows-sys"].manifest["dependencies"] = {
        "core": { "path": packages["core"].path },
        "compiler_builtins": {
            "path": packages["compiler_builtins"].path,
            "features": ["compiler-builtins"],
        }
    }

    # `rust-analyzer` *really* hates `compile_error!`s
    (library_dst_root / "windows-sys" / "src" / "lib.rs").write_text("#![no_std]", encoding="utf-8")

    # Manually add OS-specific dependencies after filtering
    packages["std"].manifest["dependencies"]["elpytios-abi"] = {
        "path": str(elpytios_abi_root),
        "features": ["sysroot-dep"],
    }

    for package in packages.values():
        package.file.write_text(package.manifest.as_string(), encoding="utf-8")

    info = tomlkit.document()
    info["fetcher-version"] = std_fetcher_version
    info["rustc-version"] = rustc_version
    (library_dst_root / "fetch_info").write_text(info.as_string(), encoding="utf-8")

def needs_fetch_std() -> bool:
    try:
        info = tomlkit.parse((library_dst_root / "fetch_info").read_text(encoding="utf-8"))
        return info["fetcher-version"] != std_fetcher_version or info["rustc-version"] != rustc_version
    except:
        return True

def build_std(_args):
    if needs_fetch_std():
        fetch_std(_args)

    env = os.environ.copy()
    if (rustflags := env.get("RUSTFLAGS")):
        rustflags += " -Awarnings -Zforce-unstable-if-unmarked"
    else:
        env["RUSTFLAGS"] = "-Awarnings -Zforce-unstable-if-unmarked"

    tmp = sysroot / "out"
    tmp.mkdir(parents=True, exist_ok=True)

    subprocess.run(
        [
            "cargo", "rustc",
            "--package", "std",
            "--target", root / "target-specs" / "x86_64-unknown-elpytios.json",
            "--profile", "release",
            "--features", "compiler-builtins-mem",
            "--target-dir", tmp,
        ],
        cwd=root,
        env=env,
        stdout=None,
        stderr=None,
        check=True
    )

    shutil.rmtree(sysroot_libdir, ignore_errors=True)
    sysroot_libdir.mkdir(parents=True)

    rlibs = (tmp / "x86_64-unknown-elpytios" / "release" / "deps").glob("lib*.rlib")
    candidates: dict[str, Path] = dict()

    for rlib in rlibs:
        name = rlib.name
        try:
            name = name[:name.index("-")]
        except ValueError:
            name = name[:name.index(".rlib")]

        if name not in candidates or candidates[name].stat().st_mtime < rlib.stat().st_mtime:
            candidates[name] = rlib

    for rlib in candidates.values():
        shutil.copy(rlib, sysroot_libdir)

def clean_std(_args):
    shutil.rmtree(library_dst_root, ignore_errors=True)
    shutil.rmtree(sysroot, ignore_errors=True)

    for item in std_dir_items:
        path = elpytios_std_root / item
        if path.exists():
            if path.is_dir():
                shutil.rmtree(path)
            else:
                path.unlink()

runner_root = root / "qemu"
runner_esp = runner_root / "esp"
runner_boot_dir = runner_esp / "EFI" / "BOOT"
runner_boot_file = runner_boot_dir / "BOOTX64.efi"
runner_fs = runner_root / "disk.qcow2"
runner_ovmf = runner_root / "OVMF"

def create_file_qemu(_args):
    subprocess.run(
        [
            "qemu-img", "create",
            "-f", "qcow2",
            runner_fs, "10G",
        ],
        stdout=None,
        stderr=None,
        check=True
    )

    runner_esp.mkdir(parents=True, exist_ok=True)

def run_qemu(args):
    if not runner_fs.exists():
        create_file_qemu(args)

    accel = "tcg"
    match sys.platform:
        case "win32": accel = "whpx"
        case "linux": accel = "kvm"

    profile = "bootloader_debug" if args.debug else "bootloader"
    subprocess.run(
        [
            "cargo", "rustc",
            "--package", "elpytios-kernel",
            "--bin", "elpytios-kernel",
            "--target", "x86_64-unknown-none",
            "--profile", profile,
            
        ],
        cwd=root,
        stdout=None,
        stderr=None,
        check=True
    )

    subprocess.run(
        [
            "cargo", "rustc",
            "--package", "elpytios-bootloader",
            "--bin", "elpytios-bootloader",
            "--target", "x86_64-unknown-uefi",
            "--profile", profile,
        ],
        cwd=root,
        stdout=None,
        stderr=None,
        check=True
    )

    runner_boot_dir.mkdir(parents=True, exist_ok=True)
    shutil.copy(root / "target" / "x86_64-unknown-uefi" / profile / "elpytios-bootloader.efi", runner_boot_file)

    subprocess.run(
        [
            f"qemu-system-{platform.machine().replace("AMD64", "x86_64")}",
            "-accel", accel,
            "-drive", f"if=pflash,format=raw,readonly=on,file={runner_ovmf / "OVMF_CODE.4m.fd"}",
            "-drive", f"if=pflash,format=raw,readonly=on,file={runner_ovmf / "OVMF_VARS.4m.fd"}",
            "-drive", f"format=raw,file=fat:rw:{runner_esp}",
            "-drive", f"format=qcow2,file={runner_fs}",
            "-machine", "q35",
            "-m", "4G",
            "-device", "virtio-vga",
            "-vga", "virtio",
            "-monitor", "stdio",
            *(["-s"] if args.debug else []),
        ],
        stdout=None,
        stderr=None,
        check=True
    )

def init_buildsystem(_args):
    if sys.platform == "win32":
        for wrap in ["rustc-sysroot"]:
            subprocess.run(
                ["rustc", root / "command-wrapper.rs", "-o", root / f"{wrap}.exe"],
                cwd=root, stdout=None, stderr=None, check=True
            )

    build_std(_args)

def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="cmd", required=True)

    # `x init`
    init = sub.add_parser("init")
    init.set_defaults(func=init_buildsystem)

    # `x std`
    std = sub.add_parser("std")
    std_sub = std.add_subparsers(dest="std_cmd", required=True)

    std_sub.add_parser("fetch").set_defaults(func=fetch_std)
    std_sub.add_parser("build").set_defaults(func=build_std)
    std_sub.add_parser("clean").set_defaults(func=clean_std)

    # `x qemu`
    qemu = sub.add_parser("qemu")
    qemu.set_defaults(func=run_qemu) # Default to `x qemu run`
    qemu_sub = qemu.add_subparsers(dest="qemu_cmd")

    qemu_sub.add_parser("create-file").set_defaults(func=create_file_qemu)
    qemu_run = qemu_sub.add_parser("run")
    qemu_run.add_argument("--debug", action="store_true")
    qemu_run.set_defaults(func=run_qemu)

    args = parser.parse_args()
    args.func(args)

if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as e:
        if (err := e.stderr):
            print(err)
        exit(1)
    except:
        exit(1)