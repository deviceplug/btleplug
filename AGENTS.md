# btleplug

Last verified: 2026-03-08

## Tech Stack

- Language: Rust (edition 2024)
- Platforms: Windows (WinRT), macOS/iOS (CoreBluetooth), Linux (BlueZ), Android (JNI)
- Async runtime: Tokio
- Testing: `cargo test`, integration tests require BLE hardware/virtual peripheral

## Commands

- `cargo build` -- Build for host platform
- `cargo test` -- Run unit tests
- `cargo test --test '*' -- --ignored` -- Run integration tests (requires test peripheral)
- `scripts/run-jni-tests.sh` -- Compile Java sources and run JNI host tests on host JVM
- `scripts/run-integration-tests.sh` -- Run BLE integration tests (requires test peripheral)
- `scripts/run-integration-tests-android.sh` -- Run Android integration tests
- `scripts/build-java.sh` -- Build Java/Android components

## Project Structure

- `src/api/` -- Public BLE API traits (Manager, Central, Peripheral)
- `src/bluez/` -- Linux BlueZ backend
- `src/corebluetooth/` -- macOS/iOS CoreBluetooth backend
- `src/droidplug/` -- Android JNI backend
- `src/winrtble/` -- Windows WinRT backend
- `src/common/` -- Shared utilities (non-Linux platforms)
- `src/platform/` -- Platform-specific type exports
- `tests/` -- Integration test suite (see `tests/CLAUDE.md`)
- `test-peripheral/` -- BLE test peripheral implementations (see `test-peripheral/CLAUDE.md`)
- `scripts/` -- Build and test automation scripts

## Feature Flags

- `serde` -- Enable serde serialization for BLE types
- `jni-host-tests` -- Enable host-side JNI testing (non-Android only). Brings in `jni/invocation` and `once_cell` as optional deps. Compiles `droidplug::jni_utils` on the host for unit testing without an Android device.

## Conventions

- Each platform backend is a separate module, conditionally compiled via `cfg`
- Public API is defined as traits in `src/api/`; backends implement these traits
- Platform-specific types are re-exported through `src/platform/`

## Boundaries

- Never import backend modules (`bluez`, `corebluetooth`, `droidplug`, `winrtble`) directly from outside `src/`; use `platform` re-exports
- `tests/` integration tests are all `#[ignore]` -- they require a running test peripheral
