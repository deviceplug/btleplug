# Android Integration Test Runbook

## Current Status (2026-03-01)

The Android test infrastructure is **fully built and deploying to device**. The pipeline works end-to-end:

1. AAR builds ✅
2. Rust cdylib cross-compiles for aarch64-linux-android ✅
3. Kotlin instrumentation tests compile ✅
4. App installs on Pixel 3a XL, native lib loads, `initBtleplug()` succeeds ✅
5. First test (`testDiscoverPeripheralByName`) runs but **panics at scan timeout** because no test peripheral is advertising ❌

The SIGABRT in logcat is a Rust panic from `peripheral_finder::find_and_connect()` timing out after 10 seconds. This is expected — the tests need a BLE test peripheral.

## What's Needed to Run Tests

### 1. Start the Zephyr test peripheral

See `test-peripheral/README.md` for setup and build instructions. The firmware runs on a Zephyr-supported BLE board (e.g., nRF52840) and advertises as "btleplug-test" with the test GATT profile.

### 2. Run the Android tests

```bash
ANDROID_HOME="$HOME/Library/Android/sdk" ./scripts/run-integration-tests-android.sh
```

Or to customize:
```bash
ANDROID_HOME="$HOME/Library/Android/sdk" \
BTLEPLUG_TEST_PERIPHERAL="btleplug-test" \
TARGET_ARCH=arm64-v8a \
./scripts/run-integration-tests-android.sh
```

### 3. Debug failures

**View crash logs:**
```bash
adb logcat -c   # clear first
# run tests
adb logcat -d | grep -iE "FATAL|signal|panic|btleplug|AndroidRuntime|UnsatisfiedLink"
```

**View test report:**
Open `tests/android/build/reports/androidTests/connected/debug/index.html`

**Run Gradle with more detail:**
```bash
cd tests/android
JAVA_HOME="/opt/homebrew/opt/openjdk@17" \
ANDROID_HOME="$HOME/Library/Android/sdk" \
./gradlew connectedAndroidTest --info
```

## Known Issues / Gotchas

### Rust panics crash the entire instrumentation process
On Android, a Rust panic in any test function calls `abort()`, which kills the process and stops ALL remaining tests. Unlike desktop (where each test is a separate process), all 28 Android tests share one process.

**Possible fix:** Wrap each JNI test function in `std::panic::catch_unwind()` and convert panics to JNI exceptions instead of aborting. This would let failing tests report as JUnit failures without crashing. Change in `tests/android/rust/src/lib.rs`:

```rust
fn run_test(env: &JNIEnv, f: impl std::future::Future<Output = ()>) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime().block_on(f);
    }));
    if let Err(panic) = result {
        let msg = if let Some(s) = panic.downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic.downcast_ref::<String>() {
            s.clone()
        } else {
            "test panicked".to_string()
        };
        env.throw_new("java/lang/AssertionError", &msg).ok();
    }
}
```

The `jni_test!` macro would need to pass `env` through.

### BTLEPLUG_TEST_PERIPHERAL env var doesn't propagate to device
The env var is read in Rust code on the Android device, but `std::env::var()` won't see host env vars. Options:
- Hardcode the default name "btleplug-test" (current behavior, works)
- Pass via `adb shell setprop` or instrumentation arguments

### BLE permissions on Android 12+
The script grants permissions via `adb shell pm grant` and the test uses `GrantPermissionRule`. Both approaches should work. If permissions fail, manually grant:
```bash
adb shell pm grant com.nonpolynomial.btleplug.test android.permission.BLUETOOTH_SCAN
adb shell pm grant com.nonpolynomial.btleplug.test android.permission.BLUETOOTH_CONNECT
adb shell pm grant com.nonpolynomial.btleplug.test android.permission.ACCESS_FINE_LOCATION
```

### jni version mismatch (FIXED)
btleplug's Cargo.toml previously said `jni = "0.22.1"` but the droidplug code uses jni 0.19 API. Fixed by pinning to `jni = "0.19.0"`.

### Fat AAR duplicate classes (FIXED)
The droidplug AAR bundles jni-utils classes. The test `build.gradle.kts` must NOT also depend on `jni-utils` as a Maven dependency.

## Architecture Overview

```
tests/
├── common/
│   ├── gatt_uuids.rs          # UUID constants
│   ├── peripheral_finder.rs    # BLE discovery/connection helpers
│   ├── test_cases.rs          # 28 async test bodies (shared)
│   └── mod.rs
├── test_*.rs                  # 28 desktop wrappers (#[tokio::test])
└── android/
    ├── rust/                  # cdylib crate
    │   ├── Cargo.toml
    │   └── src/lib.rs         # JNI exports calling test_cases::*
    ├── build.gradle.kts       # Android app project
    ├── settings.gradle.kts
    ├── src/
    │   ├── main/
    │   │   ├── AndroidManifest.xml
    │   │   └── kotlin/.../
    │   │       ├── NativeTests.kt      # external fun declarations
    │   │       └── TestHostActivity.kt
    │   └── androidTest/
    │       └── kotlin/.../
    │           └── BleIntegrationTest.kt  # 28 @Test methods
    ├── libs/                  # AAR copied here by build script
    └── src/main/jniLibs/      # .so copied here by build script

scripts/
├── run-integration-tests.sh          # Desktop runner
└── run-integration-tests-android.sh  # Android runner
```

## Commits on ble-integration-test branch

```
d21c37a fix: resolve build issues for Android integration tests
186cc4c fix: fix droidplug compilation errors for Android target
0838d84 fix: remove @Override on hidden API onConnectionUpdated
61004e9 docs: update tests/CLAUDE.md for Android test infrastructure
994a012 feat: add Android integration test runner script
05dc75f feat: add Android instrumentation test project
884c1d8 feat: add Android integration test native library
59f305a refactor: extract integration test bodies into shared test_cases module
```

## Next Steps

1. **Start the Zephyr test peripheral** so the scan finds "btleplug-test"
2. **Implement `catch_unwind`** in the JNI test wrapper so panics become JUnit failures instead of process crashes
3. **Run tests** and iterate on any BLE-specific Android failures
4. **Squash/clean commits** if desired before merging to master
