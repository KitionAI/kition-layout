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
