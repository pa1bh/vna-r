#!/usr/bin/env bash
# Build the browser viewer (docs/09-wasm-proef.md) into target/web: the GUI as
# WebAssembly, the JavaScript glue wasm-bindgen writes for it, and the page.
# That folder is the whole site: three static files, to be copied to a web server.
# Needs the Rust target once: rustup target add wasm32-unknown-unknown
# wasm-bindgen's command line tool must be the version of the wasm-bindgen
# crate in Cargo.lock; it is built into target/web-tools when it is not there.
# Run from anywhere; serve the result with: python3 -m http.server -d target/web 8080
set -euo pipefail
cd "$(dirname "$0")/.."

out=target/web
tools=target/web-tools
bindgen=$tools/bin/wasm-bindgen
version=$(awk '/^name = "wasm-bindgen"$/ { getline; gsub(/[^0-9.]/, ""); print; exit }' Cargo.lock)

if [ ! -x "$bindgen" ] || [ "$("$bindgen" --version | awk '{ print $2 }')" != "$version" ]; then
    cargo install wasm-bindgen-cli --version "$version" --locked --root "$tools"
fi

cargo build --release --target wasm32-unknown-unknown -p vna-gui
rm -rf "$out"
mkdir -p "$out"
"$bindgen" --target web --no-typescript --out-dir "$out" target/wasm32-unknown-unknown/release/vna-gui.wasm
# The page asks for the two files under the fingerprint of this build, so a
# browser never pairs an old one from its cache with a new one.
build=$(cat "$out/vna-gui.js" "$out/vna-gui_bg.wasm" | cksum | awk '{ print $1 }')
sed "s/@BUILD@/$build/" crates/vna-gui/web/index.html > "$out/index.html"
ls -lh "$out"
