#!/usr/bin/env bash
# Build the Android live wallpaper (no Gradle): android/build.sh [VERSION] [OUT.apk]
# Needs a JDK 17 and the Android SDK (platform + build-tools): JAVA_HOME and
# ANDROID_HOME, or ~/.local/jdk17 and ~/.local/android-sdk. Signs with the key
# in local/android/ (made on first use; keep it: updates need the same key).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
SDK="${ANDROID_HOME:-$HOME/.local/android-sdk}"
export JAVA_HOME="${JAVA_HOME:-$HOME/.local/jdk17}"
export PATH="$JAVA_HOME/bin:$PATH"
BT="$(ls -d "$SDK"/build-tools/* | sort -V | tail -n1)"
PLAT="$(ls -d "$SDK"/platforms/android-* | sort -V | tail -n1)/android.jar"
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
cp "$B/base.apk" "$B/unsigned.apk"
(cd "$B/dex" && zip -q ../unsigned.apk classes.dex)
"$BT/zipalign" -p -f 4 "$B/unsigned.apk" "$B/aligned.apk"
KS="${XMAS_KEYSTORE:-../local/android/xmas-tui.jks}"
PASS="${XMAS_KEYPASS:-../local/android/keystore.pass}"
if [[ ! -f "$KS" ]]; then
  mkdir -p "$(dirname "$KS")"
  head -c 48 /dev/urandom | base64 | tr -dc 'A-Za-z0-9' | head -c 28 > "$PASS"
  chmod 600 "$PASS"
  keytool -genkeypair -keystore "$KS" -alias xmas -keyalg RSA -keysize 2048 -validity 10000 \
    -dname "CN=xmas-tui" -storepass:file "$PASS" -keypass:file "$PASS" >/dev/null
fi
mkdir -p "$(dirname "$OUT")"
"$BT/apksigner" sign --ks "$KS" --ks-key-alias xmas --ks-pass "file:$PASS" --out "$OUT" "$B/aligned.apk"
"$BT/apksigner" verify "$OUT"
echo "built $OUT ($VER, code $CODE)"
