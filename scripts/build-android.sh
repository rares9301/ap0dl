#!/usr/bin/env bash
# Build the Anyomi Android app, including both Rust JNI libraries.
set -euo pipefail
cd "$(dirname "$0")/.."

export ANDROID_HOME="${ANDROID_HOME:-/opt/android-sdk}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/27.2.12479018}"

command -v cargo-ndk >/dev/null || {
    echo "cargo-ndk is required: cargo install cargo-ndk" >&2
    exit 1
}
rustup target list --installed | grep -qx aarch64-linux-android || {
    echo "install the Android target: rustup target add aarch64-linux-android x86_64-linux-android" >&2
    exit 1
}
rustup target list --installed | grep -qx x86_64-linux-android || {
    echo "install the Android target: rustup target add aarch64-linux-android x86_64-linux-android" >&2
    exit 1
}

cargo ndk -t arm64-v8a -t x86_64 \
    -o android/app/src/main/jniLibs \
    build -p ap0dl-mobile --release

cd android
./gradlew :app:assembleRelease

apk="$PWD/app/build/outputs/apk/release/app-release.apk"
echo
echo "Built $apk"
echo "Install with: adb install -r '$apk'"