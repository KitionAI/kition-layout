#!/bin/sh
# Builds the Layout engine (bindings/wasm) to WebAssembly into public/engine, where the
# editor loads it from its own package. Needs wasm-pack.
set -eu
web="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$web/.." && pwd)"
if toolchain="$(rustc --print sysroot 2>/dev/null)" && [ -d "$toolchain/lib" ]; then
  export DYLD_FALLBACK_LIBRARY_PATH="$toolchain/lib${DYLD_FALLBACK_LIBRARY_PATH:+:$DYLD_FALLBACK_LIBRARY_PATH}"
fi
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo/target}"
cd "$repo"
# On CI a job that did not set wasm-pack up (Kition's kit CI's Rust job, where the convergence
# test builds the engine) fetches the release Kition's CI uses.
if ! command -v wasm-pack >/dev/null 2>&1 && [ -n "${CI:-}" ]; then
  version="${WASM_PACK_VERSION:-v0.13.1}"
  bin="${CARGO_HOME:-$HOME/.cargo}/bin"
  mkdir -p "$bin"
  if [ "$(uname -s)-$(uname -m)" = Linux-x86_64 ]; then
    curl -fsSL "https://github.com/rustwasm/wasm-pack/releases/download/$version/wasm-pack-$version-x86_64-unknown-linux-musl.tar.gz" |
      tar -xz --strip-components=1 -C "$bin" "wasm-pack-$version-x86_64-unknown-linux-musl/wasm-pack"
  else
    cargo install wasm-pack --locked --version "${version#v}"
  fi
  export PATH="$bin:$PATH"
fi
wasm-pack build bindings/wasm --target web --release --no-typescript --out-dir "$web/public/engine"
rm -f "$web/public/engine/.gitignore" "$web/public/engine/package.json"
# The faces the web build does not embed, from the kition-fonts checkout next to the repository
# (or KITION_FONTS_DIR); the editor gives them to the engine before a document opens (E9):
# a CJK face for Chinese and Japanese text, and the four OFL families Kition's designs use
# (Inter, Lora, JetBrains Mono, Bricolage Grotesque).
fonts="${KITION_FONTS_DIR:-$repo/../kition-fonts}/fonts"
rm -rf "$web/public/fonts"
mkdir -p "$web/public/fonts"
copy() { # <dir> <font file> <licence file> <licence name in the package>
  if [ -f "$fonts/$1/$2" ]; then
    cp "$fonts/$1/$2" "$web/public/fonts/"
    cp "$fonts/$1/$3" "$web/public/fonts/$4"
  else
    echo "warning: no $1/$2 in the kition-fonts checkout at $fonts" >&2
  fi
}
copy droid-sans-fallback DroidSansFallbackFull.ttf NOTICE DroidSansFallback-NOTICE.txt
copy inter Inter-Variable.ttf OFL.txt Inter-OFL.txt
copy lora Lora-Variable.ttf OFL.txt Lora-OFL.txt
copy jetbrains-mono JetBrainsMono-Variable.ttf OFL.txt JetBrainsMono-OFL.txt
copy bricolage-grotesque BricolageGrotesque-Variable.ttf OFL.txt BricolageGrotesque-OFL.txt
