#!/usr/bin/env python3

import argparse
import shutil
import subprocess
import sys

from pathlib import Path

root = Path(__file__).resolve().parent
library_dst_root = root / "sys" / "rust-src"

std_dir = library_dst_root / "std"
std_manifest = std_dir / "Cargo.toml"

def fetch_std():
    sys_root = subprocess.run(
        ["rustc", "--print", "sysroot"],
        cwd=root,
        capture_output=True,
        text=True,
    )

    if sys_root.returncode != 0:
        raise RuntimeError(sys_root.stderr)

    library_src_root = (
        Path(sys_root.stdout.strip())
        / "lib"
        / "rustlib"
        / "src"
        / "rust"
        / "library"
    )

    if not library_src_root.exists():
        raise RuntimeError("Missing `rust-src`, run `rustup component add rust-src`")

    shutil.rmtree(library_dst_root, ignore_errors=True)
    shutil.copytree(library_src_root, library_dst_root)

    toml_text = std_manifest.read_text()

    lines = toml_text.splitlines()
    out = []
    inserted = False

    for line in lines:
        out.append(line)
        if line.strip() == "[dependencies]" and not inserted:
            out.append('elpytios-sys = { path = "../../" }')
            inserted = True

    if not inserted:
        raise RuntimeError("Missing [dependencies] in `std/Cargo.toml`")
    std_manifest.write_text("\n".join(out) + "\n")

    lib_rs = std_dir / "src" / "lib.rs"
    lib_rs.write_text(lib_rs.read_text().replace(
        "mod sys;",
        "extern crate elpytios_sys as sys;"
    ))

    shutil.rmtree(std_dir / "src" / "sys")

def build_std():
    build = subprocess.run(
        ["cargo", "build", "--manifest-path", std_manifest],
        cwd=root,
        check=True,
        stdout=None,
        stderr=None,
    )

    if build.returncode != 0:
        raise RuntimeError(build.stderr)

def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="cmd", required=True)

    std = sub.add_parser("std")
    std_sub = std.add_subparsers(dest="std_cmd", required=True)

    std_sub.add_parser("fetch")
    std_sub.add_parser("build")

    args = parser.parse_args()
    try:
        match args.cmd:
            case "std":
                match args.std_cmd:
                    case "fetch": fetch_std()
                    case "build": build_std()
    except Exception as e:
        print(f"{e}", file=sys.stderr)
        sys.exit(1)

if __name__ == "__main__":
    main()