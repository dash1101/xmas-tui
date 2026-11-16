#!/usr/bin/env bash
# Build the Android live wallpaper (no Gradle): android/build.sh [VERSION] [OUT.apk]
# Needs a JDK 17, the Android SDK (platform + build-tools) and NDK: JAVA_HOME,
# ANDROID_HOME and ANDROID_NDK_HOME, or ~/.local/jdk17 and ~/.local/android-sdk
# (with ndk/<version> in it); and Rust with the Android targets (rustup
# target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android). Signs with the
# release key in ~/.local/share/xmas-tui-signing/ (or XMAS_KEYSTORE and
# XMAS_KEYPASS); keep it: Android only updates the app from the same key.
# XMAS_NEW_KEY=1 makes a new key where none is (for a fork or a test build).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
SDK="${ANDROID_HOME:-$HOME/.local/android-sdk}"
export JAVA_HOME="${JAVA_HOME:-$HOME/.local/jdk17}"
export PATH="$JAVA_HOME/bin:$PATH"
# the newest build-tools and platform, unless ANDROID_BUILD_TOOLS (35.0.0) / ANDROID_PLATFORM (android-35) say
BT="$SDK/build-tools/${ANDROID_BUILD_TOOLS:-$(ls "$SDK"/build-tools | sort -V | tail -n1)}"
PLAT="$SDK/platforms/${ANDROID_PLATFORM:-$(ls -d "$SDK"/platforms/android-* | sort -V | tail -n1 | xargs basename)}/android.jar"
VER="${1:-$(sed -n 's/^version = "\(.*\)"/\1/p' ../Cargo.toml | head -n1)}"
OUT="${2:-build/xmas-tui-android.apk}"
IFS=. read -r MA MI PA <<<"$VER"
CODE=$((MA * 10000 + MI * 100 + PA))
B=build
rm -rf "$B" && mkdir -p "$B/gen" "$B/classes" "$B/dex"
"$BT/aapt2" compile --dir res -o "$B/res.zip"
"$BT/aapt2" link -o "$B/base.apk" -I "$PLAT" --manifest AndroidManifest.xml \
  --min-sdk-version 24 --target-sdk-version 35 --version-code "$CODE" --version-name "$VER" \
  --java "$B/gen" "$B/res.zip"
javac -nowarn -Xlint:-options --release 8 -classpath "$PLAT" -d "$B/classes" \
  $(find src "$B/gen" -name '*.java')
"$BT/d8" --release --min-api 24 --lib "$PLAT" --output "$B/dex" $(find "$B/classes" -name '*.class')
# the village to play in (android/native: the desktop's own village, as
# libxmas.so for each ABI; XMAS_ABIS picks which, e.g. "x86_64" for the emulator)
NDK="${ANDROID_NDK_HOME:-${ANDROID_NDK_LATEST_HOME:-$(ls -d "$SDK"/ndk/* 2>/dev/null | sort -V | tail -n1)}}"
NDKBIN="$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin"
[[ -x "$NDKBIN/clang" ]] || { echo "error: no Android NDK at $NDK (set ANDROID_NDK_HOME)" >&2; exit 1; }
for abi in ${XMAS_ABIS:-arm64-v8a armeabi-v7a x86_64}; do
  case "$abi" in
    arm64-v8a) T=aarch64-linux-android; CC=aarch64-linux-android24-clang ;;
    armeabi-v7a) T=armv7-linux-androideabi; CC=armv7a-linux-androideabi24-clang ;;
    x86_64) T=x86_64-linux-android; CC=x86_64-linux-android24-clang ;;
    *) echo "error: unknown ABI $abi" >&2; exit 1 ;;
  esac
  U="$(echo "$T" | tr 'a-z-' 'A-Z_')"
  # (16 KB pages: newer phones need the library laid out for them)
  env "CARGO_TARGET_${U}_LINKER=$NDKBIN/$CC" "CC_${T//-/_}=$NDKBIN/$CC" "AR_${T//-/_}=$NDKBIN/llvm-ar" \
      "CARGO_TARGET_${U}_RUSTFLAGS=-C link-arg=-Wl,-z,max-page-size=16384" \
      cargo build --quiet --release --manifest-path native/Cargo.toml --target "$T"
  mkdir -p "$B/lib/$abi"
  cp "native/target/$T/release/libxmas.so" "$B/lib/$abi/"
done
cp "$B/base.apk" "$B/unsigned.apk"
(cd "$B/dex" && zip -q ../unsigned.apk classes.dex)
(cd "$B" && zip -q -r unsigned.apk lib)
"$BT/zipalign" -p -f 4 "$B/unsigned.apk" "$B/aligned.apk"
KEYDIR="${XDG_DATA_HOME:-$HOME/.local/share}/xmas-tui-signing"
KS="${XMAS_KEYSTORE:-$KEYDIR/xmas-tui.jks}"
PASS="${XMAS_KEYPASS:-$KEYDIR/keystore.pass}"
if [[ ! -f "$KS" ]]; then
  if [[ "${XMAS_NEW_KEY:-}" != 1 ]]; then
    echo "error: no signing key at $KS (XMAS_NEW_KEY=1 makes a new one, which can't update an installed copy)" >&2
    exit 1
  fi
  mkdir -p "$(dirname "$KS")"
  head -c 48 /dev/urandom | base64 | tr -dc 'A-Za-z0-9' | head -c 28 > "$PASS"
  chmod 600 "$PASS"
  keytool -genkeypair -keystore "$KS" -alias xmas -keyalg RSA -keysize 2048 -validity 10000 \
    -dname "CN=xmas-tui" -storepass:file "$PASS" -keypass:file "$PASS" >/dev/null
fi
mkdir -p "$(dirname "$OUT")"
"$BT/apksigner" sign --ks "$KS" --ks-key-alias xmas --ks-pass "file:$PASS" --out "$OUT" "$B/aligned.apk"
"$BT/apksigner" verify "$OUT"
# the signing certificate's SHA-256 (release-cert.sha256 has the release key's)
CERT="$("$BT/apksigner" verify --print-certs "$OUT" | sed -n 's/.*certificate SHA-256 digest: //p' | head -n1)"
echo "signed by $CERT"
if [[ -n "${XMAS_EXPECT_CERT:-}" && "$CERT" != "$XMAS_EXPECT_CERT" ]]; then
  echo "error: not signed with the release key (expected $XMAS_EXPECT_CERT)" >&2
  exit 1
fi
echo "built $OUT ($VER, code $CODE)"
