#!/usr/bin/env bash
# Builds the Android artefacts for Aurora Player.
#
# This script has never produced an APK in the environment it was written in: that
# machine has no JDK, no Android SDK, no NDK and no `aarch64-linux-android` std, so
# the last two steps cannot run there at all. It is written as the exact,
# reproducible recipe for a machine that does have them, and every step checks its
# precondition before failing, so the first missing tool is named instead of
# producing a confusing error thirty lines later.
#
# Usage:
#   scripts/android/build.sh              # Rust .so files + assembleDebug
#   scripts/android/build.sh --lib-only   # Rust .so files only (used by Gradle)
#   scripts/android/build.sh --check      # report what is missing, build nothing
#
# Requires: rustup with the android targets, cargo-ndk, ANDROID_NDK_HOME, a JDK 17,
# and the Android SDK (for the Gradle half).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

LIB_ONLY=0
CHECK_ONLY=0
for arg in "$@"; do
  case "$arg" in
    --lib-only) LIB_ONLY=1 ;;
    --check) CHECK_ONLY=1 ;;
    -h|--help)
      sed -n '2,20p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "unknown option: $arg" >&2
      exit 2
      ;;
  esac
done

missing=0
report() {
  local name="$1" hint="$2"
  if [ -n "${3:-}" ]; then
    printf '  ok      %-16s %s\n' "$name" "$3"
  else
    printf '  MISSING %-16s %s\n' "$name" "$hint"
    missing=1
  fi
}

echo "checking the Android toolchain"
NDK="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}"
if [ -z "$NDK" ] && [ -n "${ANDROID_HOME:-}" ] && [ -d "$ANDROID_HOME/ndk" ]; then
  NDK="$(ls -d "$ANDROID_HOME"/ndk/* 2>/dev/null | sort -V | tail -1)"
fi
report "rustup" "https://rustup.rs (this project pins 1.99.0)" "$(command -v rustup || true)"
report "cargo-ndk" "cargo install cargo-ndk" "$(command -v cargo-ndk || true)"
report "NDK" "set ANDROID_NDK_HOME to an r26+ NDK" "$NDK"
report "java 17" "a JDK 17 on PATH (or JAVA_HOME)" "$(command -v java || true)"

if ! rustup target list --installed 2>/dev/null | grep -qx aarch64-linux-android; then
  report "aarch64 target" "rustup target add aarch64-linux-android"
fi

if [ "$missing" = 1 ]; then
  echo
  echo "cannot build for Android here; see android/README.md" >&2
  # --check is allowed to report and exit 0 so CI can inspect the gap.
  [ "$CHECK_ONLY" = 1 ] && exit 0
  exit 1
fi
[ "$CHECK_ONLY" = 1 ] && exit 0

# The Rust core. `--features jni-bridge` is what exports the four JNI symbols;
# without it the library has no entry points and Kotlin's nativeCreate would
# resolve to nothing at run time.
JNILIBS="$REPO_ROOT/android/app/src/main/jniLibs"
mkdir -p "$JNILIBS"
export ANDROID_NDK_HOME="$NDK"
echo "building libaurora_mobile.so"
cargo ndk \
  --platform 26 \
  --target arm64-v8a \
  --target x86_64 \
  --output-dir "$JNILIBS" \
  build --release -p aurora-mobile --features jni-bridge

echo "wrote:"
find "$JNILIBS" -name '*.so' -printf '  %p (%s bytes)\n'

if [ "$LIB_ONLY" = 1 ]; then
  exit 0
fi

echo "assembling the APK"
cd "$REPO_ROOT/android"
if [ ! -x ./gradlew ]; then
  if command -v gradle >/dev/null 2>&1; then
    gradle assembleDebug -PskipRust
  else
    echo "no ./gradlew wrapper and no gradle on PATH" >&2
    echo "add the wrapper with: gradle wrapper --gradle-version 8.9" >&2
    exit 1
  fi
else
  ./gradlew assembleDebug -PskipRust
fi

APK=$(find "$REPO_ROOT/android/app/build/outputs/apk" -name '*debug*.apk' | head -1)
echo "APK: $APK"
