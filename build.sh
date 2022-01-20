#!/bin/sh
rustup target add x86_64-unknown-linux-musl
cargo build --locked --release --target x86_64-unknown-linux-musl

rustup target add x86_64-pc-windows-gnu
# if on Debian or similar
sudo apt install mingw-w64
cargo build --locked --release --all-features --target x86_64-pc-windows-gnu
