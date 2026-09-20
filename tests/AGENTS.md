# tests/ -- Integration Test Suite

Freshness: 2026-03-01

## Purpose

Integration tests that exercise btleplug against a real or virtual BLE peripheral running the btleplug test GATT profile. All tests are marked `#[ignore]` so they only run when explicitly requested (`cargo test --test '*' -- --ignored`).

## Structure

Each test is its own file (and therefore its own binary), ensuring process isolation. This avoids issues with CoreBluetooth and other BLE stacks that don't cleanly handle multiple connect/disconnect cycles within a single process.

- `common/` -- shared test helpers (imported via `mod common;`)
  - `gatt_uuids.rs` -- canonical UUID constants for the test GATT profile (base UUID: `XXXXXXXX-b5a3-f393-e0a9-e50e24dcca9e`)
  - `peripheral_finder.rs` -- discover, connect, and control the test peripheral
  - `test_cases.rs` -- async test bodies shared between desktop and Android harnesses
  - `mod.rs` -- also contains `find_descriptor()` helper for descriptor tests
- `test_*.rs` -- one test per file, thin wrapper calling `common::test_cases::*`
- `android/` -- Android instrumentation test project
  - `rust/` -- cdylib crate exposing test_cases as JNI functions
  - `src/androidTest/` -- Kotlin JUnit4 instrumentation tests
  - `src/main/` -- minimal app with BLE permissions and JNI declarations
  - Built and run via `scripts/run-integration-tests-android.sh`

### Test categories

- **Discovery**: `test_discover_*.rs`, `test_scan_*.rs`, `test_advertisement_*.rs`
- **Connection**: `test_connect_*.rs`, `test_reconnect_*.rs`, `test_peripheral_triggered_*.rs`
- **Read/Write**: `test_read_*.rs`, `test_write_*.rs`, `test_long_value_*.rs`, `test_characteristic_properties.rs`
- **Notifications**: `test_subscribe_*.rs`, `test_unsubscribe_*.rs`, `test_configurable_notification_*.rs`
- **Descriptors**: `test_*_descriptor*.rs`
- **Device Info**: `test_mtu_*.rs`, `test_read_rssi.rs`, `test_properties_*.rs`, `test_connection_parameters.rs`, `test_request_connection_parameters.rs`

## Contracts

- Every peripheral-backed test file uses `find_and_connect()` from `peripheral_finder.rs` to get a connected peripheral with services discovered.
- Adapter-only scenarios may use `get_adapter()` without `find_and_connect()`; they still require local adapter hardware but do not require the btleplug test peripheral.
- Tests that mutate peripheral state must call `reset_peripheral()` in setup to ensure clean state.
- Control commands are sent via the Control Point characteristic (UUID `00000101-...`) using `send_control_command()`.
- The env var `BTLEPLUG_TEST_PERIPHERAL` overrides the default peripheral name (`btleplug-test`).

## Dependencies

- Requires a running test peripheral (Bumble virtual or Zephyr hardware) -- see `test-peripheral/`.
- UUID constants in `gatt_uuids.rs` must stay in sync with the peripheral implementations in `test-peripheral/zephyr/src/gatt_profile.h` and `test-peripheral/bumble/test_peripheral.py`.

## Invariants

- Each `test_*.rs` file contains exactly one `#[tokio::test]` function marked `#[ignore]`.
- Each `test_*.rs` is a thin wrapper delegating to `common::test_cases::*` — add new test logic to `test_cases.rs`.
- One test per file ensures process isolation — never put multiple tests in the same file.
- Tests must not depend on execution order; each test connects independently.
- The scan timeout is 10 seconds (hardcoded in `peripheral_finder.rs`).
- When adding a new test, also add the corresponding JNI export in `android/rust/src/lib.rs`, native declaration in `NativeTests.kt`, and `@Test` in `BleIntegrationTest.kt`.
- Adapter-only tests require local adapter hardware but do not require the btleplug test peripheral; their desktop assertions are target-specific because CoreBluetooth and ordinary Android intentionally return `Ok(None)`.
