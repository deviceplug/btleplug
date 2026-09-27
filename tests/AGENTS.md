# tests/ -- Integration Test Suite

Freshness: 2026-09-27

## Purpose

Integration tests that exercise btleplug against the Zephyr test peripheral running the btleplug test GATT profile. All tests are marked `#[ignore]` so they only run when explicitly requested (`cargo test --test '*' -- --ignored`).

## Structure

Each test is its own file (and therefore its own binary), ensuring process isolation. This avoids issues with CoreBluetooth and other BLE stacks that don't cleanly handle multiple connect/disconnect cycles within a single process.

- `common/` -- shared test helpers (imported via `mod common;`)
  - `gatt_uuids.rs` -- canonical UUID constants for the test GATT profile (base UUID: `XXXXXXXX-b5a3-f393-e0a9-e50e24dcca9e`)
  - `peripheral_finder.rs` -- discover, connect, and control the test peripheral; event helpers (`spawn_event_collector`, `wait_for_event`, `wait_for_connected`, `wait_for_rediscovery`)
  - `test_cases.rs` -- async test bodies shared between desktop and Android harnesses
  - `mod.rs` -- also contains `find_descriptor()` helper for descriptor tests
- `test_*.rs` -- one test per file, thin wrapper calling `common::test_cases::*`
- `android/` -- Android instrumentation test project
  - `rust/` -- cdylib crate exposing test_cases as JNI functions
  - `src/androidTest/` -- Kotlin JUnit4 instrumentation tests
  - `src/main/` -- minimal app with BLE permissions and JNI declarations
  - Built and run via `scripts/run-integration-tests-android.sh`

### Test categories

- **Discovery**: `test_discover_characteristics.rs`, `test_discover_peripheral_by_name.rs`, `test_discover_services.rs`, `test_discovery_with_included_service.rs`, `test_scan_*.rs`, `test_advertisement_*.rs`
- **Connection**: `test_connect_*.rs`, `test_reconnect_*.rs`, `test_peripheral_triggered_*.rs`
- **Read/Write**: `test_read_static_value.rs`, `test_read_counter_increments.rs`, `test_read_write_roundtrip.rs`, `test_write_*.rs`, `test_long_value_*.rs`, `test_characteristic_properties.rs`
- **Notifications**: `test_subscribe_*.rs`, `test_unsubscribe_*.rs`, `test_configurable_notification_*.rs`, `test_resubscribe_*.rs`, `test_mtu_sized_notification_payload.rs`
- **Descriptors**: `test_*descriptor*.rs`
- **Device Info**: `test_mtu_after_service_discovery.rs`, `test_read_rssi.rs`, `test_properties_*.rs`, `test_connection_parameters.rs`, `test_request_connection_parameters.rs`
- **Concurrency**: `test_concurrent_*.rs`, `test_discover_services_during_read.rs`
- **Errors**: `test_gatt_error_*.rs`, `test_refused_*.rs`, `test_operations_across_peripheral_triggered_disconnect.rs`
- **Retrieval/Adapter**: `test_adapter_*.rs`, `test_add_peripheral_*.rs`, `test_clear_peripherals_*.rs`, `test_retrieve_*.rs`

## Contracts

- Tests that need a connection use `find_and_connect()`; scan/advertisement tests use `get_adapter()` and scan, but still require the peripheral advertising.
- Tests that mutate peripheral state must call `reset_peripheral()` right after `find_and_connect()` to ensure clean state; tests that send `0x04` restore the default advertising set themselves.
- Control commands are sent via the Control Point characteristic (UUID `00000101-...`) using `send_control_command()`.
- The env var `BTLEPLUG_TEST_PERIPHERAL` overrides the default peripheral name (`btleplug-test`).
- Observe adapter events only through `spawn_event_collector()` (the capacity-16 broadcast drops lagged events). Drain with `try_recv()` before a phase that must not see earlier events.
- After any disconnect, call `wait_for_rediscovery()` before reconnecting (CoreBluetooth drops the peripheral on disconnect).
- Wrap concurrent and error-path operations in `tokio::time::timeout` (typically 10-15 s; shorter where the test expects a quick failure or fast path) so a regression fails instead of hanging.
- Platform `cfg` gates need a one-line reason citing backend source; never weaken an assertion without evidence.

## Dependencies

- Requires the Zephyr test peripheral running on hardware -- see `test-peripheral/`.
- UUID constants in `gatt_uuids.rs` must stay in sync with the Zephyr firmware in `test-peripheral/zephyr/src/gatt_profile.h`.

## Invariants

- Each `test_*.rs` file contains exactly one `#[tokio::test]` function marked `#[ignore]`.
- Each `test_*.rs` is a thin wrapper delegating to `common::test_cases::*` — add new test logic to `test_cases.rs`.
- One test per file ensures process isolation — never put multiple tests in the same file.
- Tests must not depend on execution order; each test connects independently.
- The scan timeout is 15 seconds (hardcoded in `peripheral_finder.rs`).
- When adding a new test, also add the corresponding JNI export in `android/rust/src/lib.rs`, native declaration in `NativeTests.kt`, and `@Test` in `BleIntegrationTest.kt`. Tests backed by APIs Android does not support (e.g. `test_retrieve_connected_peripheral_by_service`, `test_retrieve_connected_peripheral_by_identifier`) and macOS-only tests intentionally have no JNI export.
- Adapter-only tests require local adapter hardware but do not require the btleplug test peripheral; their desktop assertions are target-specific because CoreBluetooth and ordinary Android intentionally return `Ok(None)`.
- Each test must finish within `scripts/run-integration-tests.sh`'s 40 s per-test `TIMEOUT` (binaries are prebuilt outside it); on Windows the script also runs the ignored `winrtble::adapter::cleanup_tests` radio lib tests.
- The ignored CoreBluetooth lib tests in `src/corebluetooth/internal/tests.rs` (`#[ignore = "requires CoreBluetooth (creates a real CBCentralManager)"]`) are run manually on macOS with `cargo test --lib corebluetooth::internal::tests -- --ignored` (needs Bluetooth permission, no peripheral).
