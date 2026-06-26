.PHONY: init run run_debug fetch_std build_std

init:
	x init

run:
	x qemu run

run_debug:
	x qemu run --debug

fetch_std:
	x std fetch

build_std:
	x std build