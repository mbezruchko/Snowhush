#!/bin/zsh
# Builds the site's simulation for Autumn Leaves, Rain and Cherry Blossom: the core and the Linux shaders as
# site/engine.wasm.
set -e
cd "$(dirname "$0")"
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/snowhush_site_engine.wasm ../site/engine.wasm
ls -l ../site/engine.wasm
