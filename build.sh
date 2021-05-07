#!/bin/sh
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
strip target/release/KIT-ILIAS-downloader

rustup target add x86_64-pc-windows-gnu
# if on Debian or similar
sudo apt install mingw-w64
cargo build --release --all-features --target x86_64-pc-windows-gnu
strip target/x86_64-pc-windows-gnu/release/KIT-ILIAS-downloader.exe
