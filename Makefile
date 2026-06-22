.PHONY: init run fetch_std build_std

init:
	x init

run:
	x qemu run

fetch_std:
	x std fetch

build_std:
	x std build