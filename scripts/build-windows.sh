#!/usr/bin/env bash
# Cross-compile the Windows build from Linux and package it as a zip.
# Needs: rustup target add x86_64-pc-windows-gnullvm, and llvm-mingw
# (https://github.com/mstorsjo/llvm-mingw) unpacked to ~/.local/llvm-mingw
# (or point LLVM_MINGW at it).
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$DIR"
LLVM_MINGW="${LLVM_MINGW:-$HOME/.local/llvm-mingw}"
[[ -x "$LLVM_MINGW/bin/x86_64-w64-mingw32-clang" ]] || { echo "llvm-mingw not found at $LLVM_MINGW (set LLVM_MINGW)"; exit 1; }
export CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER="$LLVM_MINGW/bin/x86_64-w64-mingw32-clang"
export CC_x86_64_pc_windows_gnullvm="$LLVM_MINGW/bin/x86_64-w64-mingw32-clang"
export AR_x86_64_pc_windows_gnullvm="$LLVM_MINGW/bin/llvm-ar"
cargo build --release --target x86_64-pc-windows-gnullvm
OUT="$DIR/dist/xmas-tui-windows"
rm -rf "$OUT" "$OUT.zip"
mkdir -p "$OUT"
cp target/x86_64-pc-windows-gnullvm/release/xmas-tui.exe "$OUT/"
cp packaging/README-Windows.txt "$OUT/README.txt"
cp config.example.toml "$OUT/"
cp assets/fonts/LICENSE-*.txt "$OUT/"
# Windows-friendly line endings for the text files
for f in "$OUT"/*.txt "$OUT"/*.toml; do sed -i 's/$/\r/' "$f"; done
(cd "$DIR/dist" && zip -qr xmas-tui-windows.zip xmas-tui-windows)
echo "built $DIR/dist/xmas-tui-windows.zip"
