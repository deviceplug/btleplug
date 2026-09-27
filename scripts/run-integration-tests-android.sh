#!/usr/bin/env bash
#
# Build and run btleplug integration tests on a connected Android device.
#
# Prerequisites:
#   - cargo-ndk: cargo install cargo-ndk
#   - Android NDK installed (via Android Studio or sdkmanager)
#   - Android SDK with platform matching compileSdk (34)
#   - A connected Android device with BLE (adb devices must show it)
#   - The btleplug test peripheral running and advertising
#
# Usage:
#   ./scripts/run-integration-tests-android.sh
#
# Environment:
#   ANDROID_HOME / ANDROID_SDK_ROOT  - Android SDK path
#   ANDROID_NDK_HOME                 - Android NDK path (optional, auto-detected)
#   BTLEPLUG_TEST_PERIPHERAL         - peripheral name (default: btleplug-test)
#   TARGET_ARCH                      - NDK target (default: arm64-v8a)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ANDROID_TEST_DIR="$PROJECT_ROOT/tests/android"
RUST_CRATE_DIR="$ANDROID_TEST_DIR/rust"
DROIDPLUG_JAVA_DIR="$PROJECT_ROOT/src/droidplug/java"
TARGET_ARCH="${TARGET_ARCH:-arm64-v8a}"

# Map Android ABI to Rust target triple
case "$TARGET_ARCH" in
    arm64-v8a)   RUST_TARGET="aarch64-linux-android" ;;
    armeabi-v7a) RUST_TARGET="armv7-linux-androideabi" ;;
    x86_64)      RUST_TARGET="x86_64-linux-android" ;;
    x86)         RUST_TARGET="i686-linux-android" ;;
    *) echo "ERROR: unsupported TARGET_ARCH: $TARGET_ARCH"; exit 1 ;;
esac

echo "=== btleplug Android integration tests ==="
echo "Target: $TARGET_ARCH ($RUST_TARGET)"
echo ""

# ── Check prerequisites ─────────────────────────────────────────────

if ! command -v cargo-ndk &>/dev/null; then
    echo "ERROR: cargo-ndk not found. Install with: cargo install cargo-ndk"
    exit 1
fi

if ! command -v adb &>/dev/null; then
    echo "ERROR: adb not found. Install the Android SDK platform-tools."
    exit 1
fi

DEVICE_COUNT=$(adb devices | grep -cw 'device' || true)
if [[ "$DEVICE_COUNT" -lt 1 ]]; then
    echo "ERROR: no Android device connected. Check 'adb devices'."
    exit 1
fi

# Resolve ANDROID_HOME
ANDROID_HOME="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
if [[ -z "$ANDROID_HOME" ]]; then
    # Common default locations
    if [[ -d "$HOME/Library/Android/sdk" ]]; then
        ANDROID_HOME="$HOME/Library/Android/sdk"
    elif [[ -d "$HOME/Android/Sdk" ]]; then
        ANDROID_HOME="$HOME/Android/Sdk"
    else
        echo "ERROR: ANDROID_HOME not set and SDK not found in default locations."
        exit 1
    fi
fi
export ANDROID_HOME
echo "ANDROID_HOME: $ANDROID_HOME"

# ── Step 1: Build btleplug AAR ──────────────────────────────────────

echo ""
echo ">>> Step 1/4: Building btleplug AAR..."
"$SCRIPT_DIR/build-java.sh"

# Find the built AAR
AAR_FILE=$(find "$DROIDPLUG_JAVA_DIR/build/outputs/aar" -name '*.aar' | head -1)
if [[ -z "$AAR_FILE" ]]; then
    echo "ERROR: btleplug AAR not found after build."
    exit 1
fi
echo "    AAR: $AAR_FILE"

# ── Step 2: Build Rust cdylib ───────────────────────────────────────

echo ""
echo ">>> Step 2/4: Building Rust test library for $TARGET_ARCH..."
cargo ndk -t "$TARGET_ARCH" build \
    --manifest-path "$RUST_CRATE_DIR/Cargo.toml" \
    --release

SO_FILE="$RUST_CRATE_DIR/target/$RUST_TARGET/release/libbtleplug_android_tests.so"
if [[ ! -f "$SO_FILE" ]]; then
    echo "ERROR: .so not found at $SO_FILE"
    exit 1
fi
echo "    .so: $SO_FILE"

# ── Step 3: Copy artifacts into Gradle project ──────────────────────

echo ""
echo ">>> Step 3/4: Copying artifacts..."

# Copy AAR
mkdir -p "$ANDROID_TEST_DIR/libs"
cp "$AAR_FILE" "$ANDROID_TEST_DIR/libs/"
echo "    Copied AAR → tests/android/libs/"

# Copy .so
JNILIBS_DIR="$ANDROID_TEST_DIR/src/main/jniLibs/$TARGET_ARCH"
mkdir -p "$JNILIBS_DIR"
cp "$SO_FILE" "$JNILIBS_DIR/"
echo "    Copied .so → tests/android/src/main/jniLibs/$TARGET_ARCH/"

# Write local.properties
echo "sdk.dir=$(cd "$ANDROID_HOME" && pwd)" > "$ANDROID_TEST_DIR/local.properties"

# ── Step 4: Run instrumentation tests ───────────────────────────────

echo ""
echo ">>> Step 4/4: Running instrumentation tests..."

# build-java.sh ran in a subprocess, so resolve the same JDK again here.
# shellcheck source=lib/java-home.sh
source "$SCRIPT_DIR/lib/java-home.sh"
JAVA_HOME="$(find_java_home)"
if [[ -z "$JAVA_HOME" ]]; then
    echo "ERROR: Could not find a JDK 17 or 21. Set JAVA_HOME and re-run."
    exit 1
fi
export JAVA_HOME
echo "    JAVA_HOME: ${JAVA_HOME:-<unset>}"

# Ensure Gradle wrapper is available
if [[ ! -f "$ANDROID_TEST_DIR/gradlew" ]]; then
    # Use the wrapper from the droidplug Java project
    cp "$DROIDPLUG_JAVA_DIR/gradlew" "$ANDROID_TEST_DIR/"
    cp -r "$DROIDPLUG_JAVA_DIR/gradle" "$ANDROID_TEST_DIR/"
fi

cd "$ANDROID_TEST_DIR"
chmod +x gradlew

# Keep the screen on while connected via USB. Android suspends unfiltered
# BLE scans when the screen is off (BtGatt.ScanManager screen-off policy).
adb shell svc power stayon true
# Wake the screen in case it's currently off
adb shell input keyevent KEYCODE_WAKEUP

./gradlew connectedAndroidTest
TEST_EXIT=$?

# Restore default stay-on behavior
adb shell svc power stayon false

echo ""
echo "=== Android integration tests complete ==="
exit $TEST_EXIT
