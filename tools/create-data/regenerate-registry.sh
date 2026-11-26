#!/bin/sh
set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec cargo run --locked --manifest-path "$here/Cargo.toml" -- registry --out "$here/../../create" "$@"
