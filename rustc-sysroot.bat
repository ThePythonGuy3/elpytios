#!/usr/bin/env sh
: << 'BATCH'

@echo off
py "%~dp0rustc-sysroot" %*
exit /b %errorlevel%

BATCH

set -eu
script=$0
while [ -L "$script" ]; do
    link=$(readlink "$script")
    case $link in
        /*) script=$link ;;
        *)  script=$(CDPATH= cd -- "$(dirname -- "$script")" && pwd)/$link ;;
    esac
done

dir=$(CDPATH= cd -- "$(dirname -- "$script")" && pwd)
exec "$dir/rustc-sysroot" "$@"