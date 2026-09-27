# Plan: Zephyr Hardware Regression Test Expansion

Written: 2026-09-26
Branch: `bug-finding` (these tests are regression coverage for the fixes already on this branch)

## Context

The hardware integration suite (`tests/`, driven against the Zephyr test peripheral on an nRF52840 DK) has not grown since 2026-08-29. The last firmware change was GAP appearance (`39fddc1`); the last real test additions were retrieval, adapter address, and macOS `clear_peripherals`. Since then 0.13.0 through 0.13.3 shipped ~40 fix commits with no new integration coverage. Most of those bugs lived in concurrency, error-status, and teardown paths that the current connect → single-op → disconnect tests never reach.

The original design (`docs/design-plans/2026-02-22-ble-integration-test-system.md:83,125`) also specified service data advertising and a working `CMD_CHANGE_ADVERTISEMENTS` (0x04) rotation. Neither was built.

The Bumble virtual peripheral has never been run. It is already non-functional (it packs 47 bytes of AD structures into a 31-byte legacy advertising packet and omits flags) and has drifted from the Zephyr behaviour. It is retired in Step 2 and will be revisited once there is a concrete plan to make it useful (see "Future Work").

### Fix → coverage map

| Issue | Backend | Step | Firmware change |
|---|---|---|---|
| #488 concurrent connect/disconnect/discover waiters | CoreBluetooth | 3 | No |
| #481 deadlock on concurrent ops in one service | WinRT | 3 | No |
| #489 in-flight ops across rediscovery | CoreBluetooth | 3 | No |
| #484 stale services after dropped link; connect-when-connected | WinRT | 4 | No |
| #326 duplicate handlers on re-subscribe | WinRT | 4 | No |
| `DeviceConnected` / `DeviceDisconnected` never asserted | All | 4 | No |
| #490 disconnect mid-operation wedges queue / hides `DeviceDisconnected` | Android | 5 | No |
| `add_peripheral()` and retrieval by identifier untested | WinRT, Android, CB, BlueZ | 6 | No |
| #489 `clear_peripherals()` disconnects connected peripheral | CoreBluetooth | 7 | No |
| #492 failed reads/writes reported as success | Android | 8 | Yes |
| #471 refused subscribe never resolves | CoreBluetooth | 8 | Yes |
| #464 write-without-response order under backpressure | CoreBluetooth | 9 | Yes |
| #487 panic on characteristics for included services | CoreBluetooth | 10 | Yes |
| #483 128-bit service data UUID byte order | WinRT | 11 | Yes |
| MTU-sized notifications (firmware caps payload at 20 bytes) | All | 12 | Yes |
| #476 Windows radio handler tests never run | WinRT | 13 | No |

Not covered on hardware, with reasons: #485 (needs BlueZ < 5.62), #491 and #486 (GATT discovery failure cannot be forced from Zephyr; empty service set needs separate firmware), #482 (malformed adverts risk other stacks dropping the packet; covered by winrtble unit tests), FnAdapter race (covered by `src/droidplug/jni_utils/ops/test.rs` stress tests), `DeviceServicesModified` (Zephyr does not send Service Changed to unbonded clients; needs bonding support first).

## Execution Loop

Every step runs this loop. Do not start the next step until the current step is committed.

1. **Implement** with a `sonnet` subagent (`ed3d-plan-and-execute:task-implementor-fast`, `model: sonnet`). Use `haiku` (`ed3d-basic-agents:haiku-general-purpose`) for doc-only or mechanical steps (Steps 2 and 14 doc edits). The implementor receives the step text from this file verbatim plus the conventions below.
2. **Verify** locally before review:
   - `cargo fmt --check`
   - `cargo build --tests`
   - `cargo clippy --tests`
   - Firmware steps: `uv run west build -b nrf52840dk/nrf52840 --pristine` from `test-peripheral/zephyr/`
   - Hardware run when the board is attached: `uv run west flash --runner nrfjprog`, then `./scripts/run-integration-tests.sh <new test names>` plus the full suite for firmware steps. If the board is not attached, stop and hand the hardware run to the user before committing.
3. **Review** with an `opus` subagent (`ed3d-plan-and-execute:code-reviewer`, `model: opus`). Reviewer checks the diff against this step's text, the invariants in `tests/AGENTS.md`, and platform gating correctness.
4. **Fix** any review issues with a `sonnet` subagent (`ed3d-plan-and-execute:task-bug-fixer`, `model: sonnet`), then re-review with `opus`. Repeat until the reviewer reports zero issues.
5. **Commit** once per step with the commit message given in the step, ending with the session's Co-Authored-By trailer.

### Conventions for every test step

- Test logic goes in `tests/common/test_cases.rs`; each new test gets a thin `tests/test_<name>.rs` wrapper with exactly one `#[tokio::test]` `#[ignore = "requires BLE test peripheral"]` function.
- Unless the test is platform-gated off Android, add a `jni_test!` export in `tests/android/rust/src/lib.rs`, an `external fun` in `tests/android/src/main/kotlin/.../NativeTests.kt`, and a `@Test` in `tests/android/src/androidTest/kotlin/.../BleIntegrationTest.kt`.
- Wrap every concurrent or error-path operation in `tokio::time::timeout` so a regression fails instead of hanging. Use 10-15 second bounds.
- Tests that mutate peripheral state call `peripheral_finder::reset_peripheral()` after connecting.
- New UUIDs and opcodes go in `tests/common/gatt_uuids.rs` and `test-peripheral/zephyr/src/gatt_profile.h` together.
- Platform semantics that differ legitimately (e.g. BlueZ rejecting a concurrent `Connect` with `InProgress`) are gated with `cfg` and a one-line reason. Do not weaken an assertion to make a platform pass without flagging it to the user; an unexpected failure may be a real backend bug.
- New characteristics are appended to the end of existing services so the hardcoded `notify_svc.attrs[]` indices in `control_service.c` stay valid. Update the index table comment when `notify_svc` grows.

## Progress and Handoff

Updated: 2026-09-27, after Step 11.

### Status

| Step | Commit(s) |
|---|---|
| Plan | `85987cb` |
| 1 | `7ff431f` (pinned to v4.4.2, not v4.4.0; SDK 1.0.1) |
| 2 | `9bc6f9e` |
| 3 | `a2507c0` |
| 4 | `4429e11` |
| 5 | `dd75aea` |
| 6 | `f4034b2` |
| 7 | `05b1922` (library fix found by this step), `487a633` (stale-event fix to `test_clear_peripherals_rediscovers_device`), `20b5520` |
| 8 | `84f9dc2` (refusing CCC is an unmanaged descriptor, not the plan's managed CCC: a managed CCC leaks Zephyr's only cfg slot on rejection) |
| 9 | `b768000` (no drops or reordering observed on macOS) |
| 10 | `79fb66b` (proved against both #487 panic sites separately; handles follow service variable names, see conventions) |
| 11 | `d68ff66` (test restores the default advertising set itself; skips `wait_for_rediscovery` because the service-data event implies rediscovery on CoreBluetooth) |
| 12-14 | Not started |

Baseline before Step 1: 32/32 hardware tests passing on macOS. After Step 11: 45/45. Only macOS has been run on hardware; Windows, Linux, and Android branches are unverified.

### Working conventions learned so far

- **Firmware tooling.** Workspace topdir is `test-peripheral/` (deps in `test-peripheral/deps/`, uv venv in `test-peripheral/.venv/`). Use `test-peripheral/.venv/bin/west` (or activate the venv; fish: `activate.fish`). Build from `test-peripheral/zephyr/`: `west build -b nrf52840dk/nrf52840 --pristine`; flash: `west flash --runner nrfjprog`.
- **Hardware runs.** Use `TIMEOUT=60 ./scripts/run-integration-tests.sh <names>` until Step 13 raises the default. Run new tests at least 3 times (5 for timing-dependent ones). Write repeat loops as bash scripts in the scratchpad; the tool shell is zsh and mangles things like `echo ====`.
- **Event helpers** in `tests/common/peripheral_finder.rs`: `spawn_event_collector` (always use it; the central event broadcast has capacity 16 and silently drops lagged events), `wait_for_event(rx, timeout, what, pred)`, `wait_for_connected` (skips stale BlueZ synthetic connection events), `wait_for_rediscovery` (no-op off Apple). Drain the receiver with `try_recv()` before starting a new phase that must not see earlier events.
- **CoreBluetooth reconnect.** CoreBluetooth drops a peripheral on disconnect (issue #57); call `wait_for_rediscovery` before any reconnect after a disconnect.
- **CoreBluetooth `clear_peripherals`** disconnects a connected peripheral without emitting `DeviceDisconnected` (documented contract since f3de711).
- **No pre-emptive relaxation.** Implementors repeatedly added `cfg` relaxations for BlueZ/Android without evidence; reviewers rejected them. Gate only with backend source evidence cited in a one-line comment.
- **Housekeeping.** Any cargo command against `tests/android/rust` rewrites `tests/android/rust/Cargo.lock`; `git checkout -- tests/android/rust/Cargo.lock` afterwards. Never use `git stash` (shared stack). Keep comments in `test_cases.rs` sparse; implementors tend to over-comment.
- **ESP32-S3 build check.** The board is `esp32s3_devkitc/esp32s3/procpu` (not `devkitm`); put `test-peripheral/.venv/bin` on `PATH` so the build finds `esptool`.
- **Firmware threading.** Both boards build with `CONFIG_BT_RECV_WORKQ_BT=y`: ATT/GATT callbacks, including `CMD_RESET_STATE` (called directly from the Control Point write), run serially on the "BT RX WQ" thread. Only `periodic_notify_handler` and `disconnect_handler` run on the system workqueue.
- **RTT / J-Link.** The DK's onboard J-Link runs old firmware (V1): `JLinkRTTLogger` never finds the RTT control block, and attaching `JLinkExe` at 4 MHz SWD stops BLE advertising (tests time out in `find_and_connect()`). Don't sink time into RTT capture; prove firmware behaviour from Zephyr source and test outcomes, or use macOS PacketLogger for ATT traffic.
- **Plan text vs. source.** Check plan claims against the source before implementing: Step 6's `NotSupported("add_peripheral")` string and Step 7's `DeviceDisconnected` expectation were both wrong.
- **GATT handle order.** Static services are placed by `SORT_BY_NAME` on the `BT_GATT_SERVICE_DEFINE` variable name, not source order (current order: control, descriptor, included, notify, rw). Only relative `attrs[]` indices are safe to hardcode.
- **Regression proofs.** When a fix removed several panic or error sites, reintroduce them one at a time: an earlier site masks later ones. Add a control test (an existing test on the same broken code) to show the new test is what reaches the site.

### Findings outside this plan (report to the user; not fixed)

1. `find_and_connect()` returns as soon as the local name matches, before the scan response (manufacturer data, service UUIDs) may have arrived, so `test_properties_contain_peripheral_info` is flaky (observed 1 failure in ~40 runs).
2. CoreBluetooth `connect()` immediately after `disconnect()` fails with "Peripheral no longer available" until the device re-advertises; other backends allow it.
3. The integration tests never initialise a logger, so `RUST_LOG` has no effect.
4. The ESP32-S3 build warns `CONFIG_HEAP_MEM_POOL_SIZE` 4096 is below the required 29696 (Zephyr bumps it automatically).
5. `AdapterManager` (`src/common/adapter_manager.rs`) uses a capacity-16 broadcast and `event_stream()` silently drops `Lagged`. Measured bursts of ~130 `DeviceUpdated` within ±500 ms of a disconnect during scanning. Any `events()` consumer can lose `DeviceConnected`/`DeviceDisconnected`. One unexplained cycle-2 `DeviceDisconnected` miss in Step 5 (1/5 runs, not reproduced in 25 instrumented runs) is attributed to this.
6. WinRT does not clear `ble_services` after a peripheral-triggered disconnect, so a read afterwards may trigger an implicit reconnect; check on Windows hardware.
7. `add_peripheral`'s `NotSupported` message ("Can't add a Peripheral from a PeripheralId") does not follow the operation-name convention `retrieve_peripherals` uses.
8. The BlueZ repeat-run behaviour of `test_advertisement_service_data_128bit` relies on `device_set_service_data` clearing and re-adding service data when scanning with `duplicate_data` (checked in BlueZ master; the release that added this is unknown). An older BlueZ may send no `PropertiesChanged` on a second run.
9. BlueZ never clears `ServiceData` when later advertisements omit it, so `properties().service_data` keeps showing stale entries while the device object exists.
10. If `test_advertisement_service_data_128bit` fails before its final reconnect, the alternate set persists until the next connecting test; a scan-only test run in between (`test_advertisement_services`) fails as a knock-on.
11. Firmware: `connected(err)` returns without restarting advertising, and Zephyr v4.4.2 has no auto-resume, so a failed connection would leave the peripheral silent (behaviour predates this plan).

## Steps

### Step 1: Pin the Zephyr version

**Goal:** Reproducible firmware builds.

- Add `test-peripheral/zephyr/west.yml` (T2 star topology) pinning `zephyr` to `v4.4.2` (Zephyr SDK 1.0.1), importing only the modules the two boards need (`hal_nordic`, `cmsis`/`cmsis_6`, `hal_espressif`, plus whatever the ESP32-S3 build requires). Before writing, have a research subagent confirm v4.4.2 is still the latest stable release and verify the allowlist names against the v4.4.2 `west.yml`.
- Update `test-peripheral/README.md` and `docs/zephyr-test-peripheral-debugging.md` with the `west init -l` / `west update` workflow.
- Verify the nRF52840 build succeeds against the pinned tree.

**Commit:** `build(zephyr): Pin test peripheral firmware to Zephyr v4.4.2`

### Step 2: Retire Bumble and fix stale test docs

**Goal:** Stop documenting a second peripheral that does not work.

- Delete `test-peripheral/bumble/`.
- Remove Bumble instructions from `README.md` (~line 203), `test-peripheral/README.md` (Option B and Bumble troubleshooting), `docs/android-integration-test-runbook.md` (Option A), and `tests/AGENTS.md:44-45`.
- Rename `test-peripheral/CLAUDE.md` → `test-peripheral/AGENTS.md` (matching `1800369`), rewrite it for a single Zephyr implementation, drop the "must behave identically" invariant, and update its freshness date. Note in it that Bumble was removed in this commit and can be restored from history.
- Fix root `AGENTS.md:31-32` to point at `tests/AGENTS.md` and `test-peripheral/AGENTS.md`.
- Fix `tests/AGENTS.md:53`: the scan timeout is 15 seconds (`peripheral_finder.rs:13`).
- Note in the `tests/AGENTS.md` Android invariant that tests backed by APIs Android does not support (e.g. `test_retrieve_connected_peripheral_by_service`, macOS-only tests) intentionally have no JNI export.
- Leave historical design plans under `docs/design-plans/` unchanged.

**Commit:** `test: Retire unmaintained Bumble peripheral and fix stale test docs`

### Step 3: Concurrency regression tests

**Goal:** Cover #488, #481, #489 (rediscovery) with no firmware change.

- `test_concurrent_connect_and_discover`: after `find_and_connect()` and `disconnect()`, run `join!(connect(), connect())`, then `join!(discover_services(), discover_services())`, then `join!(disconnect(), disconnect())`, each under a timeout. Every call must complete. On macOS and Windows every call must return `Ok`. If BlueZ or Android return an in-progress error for the second call, gate that with `cfg` and require at least one `Ok` plus the expected final `is_connected()` state.
- `test_concurrent_operations_same_service`: `join!(subscribe(NOTIFY_CHAR), subscribe(INDICATE_CHAR))` and `join!(read(STATIC_READ), read(COUNTER_READ))`, all under a timeout, all `Ok`, static value correct.
- `test_discover_services_during_read`: `join!(read(COUNTER_READ), discover_services())`; both resolve `Ok`, and a follow-up `read(STATIC_READ)` succeeds.

**Commit:** `test: Add concurrent connect, discovery, and same-service operation tests`

### Step 4: Reconnect, event, and re-subscribe tests

**Goal:** Cover #484, #326, and connection events.

- `test_reconnect_after_peripheral_triggered_disconnect`: trigger 0x03, wait for disconnection, call `connect()` without an explicit `disconnect()`, `discover_services()`, read `STATIC_READ` successfully. Then call `connect()` again while connected and assert it returns `Ok` within 2 seconds (the #484 fast path).
- Extend `test_connect_and_disconnect` and `test_peripheral_triggered_disconnect` to subscribe to `adapter.events()` before `find_and_connect()` and assert `DeviceConnected(id)` then `DeviceDisconnected(id)` for the test peripheral's id, each within a timeout. Ignore events for other ids.
- `test_resubscribe_does_not_duplicate_notifications`: subscribe to `NOTIFY_CHAR` twice, start notifications, collect for ~4 seconds, assert at least 3 notifications and no repeated counter byte.

**Commit:** `test: Add reconnect, connection event, and re-subscribe regression tests`

### Step 5: Disconnect during operations

**Goal:** Cover #490.

- `test_operations_across_peripheral_triggered_disconnect`: subscribe to adapter events, send 0x03, then loop issuing `read_descriptor(READ_ONLY_DESCRIPTOR)` and `read(COUNTER_READ)` (each under a 5 second timeout) until one returns `Err`. No operation may time out. Assert `DeviceDisconnected(id)` arrives. Assert one more read after disconnection returns `Err` promptly (`Error::NotConnected` on Android). Then reconnect, rediscover, and read `STATIC_READ` successfully to prove the command queue is not wedged.

**Commit:** `test: Add disconnect-during-operation regression test`

### Step 6: `add_peripheral` and retrieval by identifier

**Goal:** Cover APIs with no integration coverage.

- `test_add_peripheral_by_address`: discover the peripheral and record its id, clear local state appropriately, call `add_peripheral(&id)`, connect, discover, read `STATIC_READ`. Windows and Android must succeed; BlueZ and CoreBluetooth must return `Error::NotSupported("add_peripheral")` (assert the contract, gated per target).
- `test_retrieve_connected_peripheral_by_identifier`: connect, then `retrieve_peripherals` with `identifiers: Some(vec![id])`; the peripheral must be returned. Desktop only; no Android export (Android returns `NotSupported`, already covered by `test_retrieve_peripherals_not_supported`).

**Commit:** `test: Cover add_peripheral and peripheral retrieval by identifier`

### Step 7: macOS `clear_peripherals` while connected

**Goal:** Cover the #489 `clear_peripherals` behaviour change.

- `test_clear_peripherals_disconnects_connected_peripheral` (macOS only): `find_and_connect()`, subscribe to events, `clear_peripherals()`, assert the peripheral left the public map, then assert it is rediscovered (it only re-advertises once the link is actually down) with no `DeviceDisconnected(id)` before that, per the `Central::clear_peripherals` doc. Finally connect the rediscovered handle and read `STATIC_READ`.

**Commit:** `test(corebluetooth): Cover clear_peripherals on a connected peripheral`

### Step 8: GATT error and refused-CCC characteristics

**Goal:** Cover #492 and #471.

Firmware:
- `ERROR_CHAR` `0x00000207` appended to the Read/Write service, `READ | WRITE`, both handlers return `BT_GATT_ERR(0x80)` (application error; avoid authentication/encryption errors, which trigger pairing on macOS and Windows).
- `REFUSED_NOTIFY_CHAR` `0x00000304` appended to the Notification service, `NOTIFY`, with a managed CCC whose write callback rejects every enable with an ATT error. Verify the exact v4.4.2 API (`BT_GATT_CCC_MANAGED` / `struct bt_gatt_ccc_managed_user_data` `cfg_write`, or a newer write-callback CCC macro) against the pinned tree before writing; earlier research on this signature was unreliable.

Tests:
- `test_gatt_error_status_is_reported`: `read(ERROR_CHAR)` and `write(ERROR_CHAR, WithResponse)` both return `Err` within a timeout; a follow-up `read(STATIC_READ)` succeeds.
- `test_refused_subscribe_returns_error`: `subscribe(REFUSED_NOTIFY_CHAR)` returns `Err` within a timeout; a follow-up `subscribe(NOTIFY_CHAR)` succeeds and receives a notification.

**Commit:** `test(zephyr): Add GATT error and refused subscription characteristics`

### Step 9: Write-without-response ordering

**Goal:** Cover #464; the existing burst test cannot observe reordering.

Firmware:
- In `write_without_resp`, keep the existing `rw_value` storage and additionally track `wwr_count`, `wwr_last_seq` (first byte of each write), and `wwr_out_of_order` (incremented when a sequence byte is not `last + 1`). Reset clears them.
- `WRITE_LOG_CHAR` `0x00000208` appended to the Read/Write service, `READ`, returning `[count_lo, count_hi, last_seq, out_of_order]`.

Tests:
- Extend `test_write_without_response_burst`: after the 50 writes, poll `WRITE_LOG_CHAR` for up to 2 seconds until the count reaches 50, then assert count == 50 and out_of_order == 0. A dropped write on any platform is a finding to report, not something to loosen.

**Commit:** `test(zephyr): Verify write-without-response delivery order`

### Step 10: Included secondary service

**Goal:** Cover #487.

Firmware:
- Secondary service `0x00000005` (`BT_GATT_SECONDARY_SERVICE`) with `INCLUDED_CHAR` `0x00000501` (`READ`, fixed value `[0x05]`).
- Include it from the Descriptor service. Include declarations must immediately follow the service declaration, so this inserts at `descriptor_svc.attrs[1]`; nothing references `descriptor_svc` indices. Verify the v4.4.2 `BT_GATT_INCLUDE_SERVICE` argument type (it takes the included service's declaration attribute, not the service struct) against the pinned tree.

Tests:
- `test_discovery_with_included_service`: `find_and_connect()` succeeds (discovery completes), all four primary services are present, and `read(STATIC_READ)` succeeds afterwards. Do not assert whether the secondary service appears in `services()`; that is backend-dependent.

Note in the step's hardware run: Windows caches GATT databases even for unpaired devices. After flashing, remove the device in Windows Bluetooth settings (or clear the cache) if discovery returns the old layout.

**Commit:** `test(zephyr): Add included secondary service to the test profile`

### Step 11: Advertisement rotation with 128-bit service data

**Goal:** Cover #483 and build the part of the original design that was deferred.

Firmware:
- Implement `CMD_CHANGE_ADVERTISEMENTS` (0x04): set an `alt_adv_pending` flag. When advertising restarts after a disconnect with the flag set, use the alternate set: advertising data = flags + complete name; scan response = 128-bit service data (`BT_DATA_SVC_DATA128`: Control Service UUID little-endian + `[0x01]`) + manufacturer data. On the next `connected()` callback, clear the flag so the following disconnect restores the default set. `CMD_RESET_STATE` also clears it.
- Replace the "deferred" comment in `control_service.c` and update the opcode description in `test-peripheral/AGENTS.md`.

Tests:
- `test_advertisement_service_data_128bit`: `find_and_connect()`, send 0x04, `disconnect()`, listen for `CentralEvent::ServiceDataAdvertisement` whose map contains `CONTROL_SERVICE` → `[0x01]` (byte-for-byte UUID match proves the byte order) within 15 seconds; also assert `properties().service_data` contains it. Add a `SERVICE_DATA_VALUE` constant to `gatt_uuids.rs`.

**Commit:** `test(zephyr): Implement advertisement rotation with 128-bit service data`

### Step 12: MTU-sized notification payloads

**Goal:** Exercise notifications larger than the default ATT payload.

Firmware:
- Grow `notify_payload` from 20 to 244 bytes (`CONFIG_BT_L2CAP_TX_MTU=247` minus the 3-byte ATT header), keeping the truncation guard.

Tests:
- `test_mtu_sized_notification_payload`: after connecting, compute `len = min(peripheral.mtu() - 4, 243)` (the Control Point write carries the opcode byte and must fit in one ATT write), set a patterned payload of that length with 0x06, subscribe to `CONFIGURABLE_NOTIFY`, start notifications, and assert a notification arrives with the exact payload. If the negotiated MTU is the default 23, the test still runs with a 19-byte payload.

**Commit:** `test(zephyr): Support and test MTU-sized notification payloads`

### Step 13: Integration script updates

**Goal:** Run the orphaned Windows hardware tests and fit the longer new tests.

- In `scripts/run-integration-tests.sh`, on Windows (`$OS == Windows_NT` or `uname` matching MINGW/MSYS), also run `cargo test --lib -- --ignored` so the `src/winrtble/adapter.rs` radio tests (#476) execute.
- Raise the default per-test `TIMEOUT` from 20 to 40 seconds (Steps 4, 5, 7, and 11 include a reconnect or rescan cycle that can exceed 20 seconds).

**Commit:** `scripts: Run Windows radio tests and raise integration test timeout`

### Step 14: Final documentation and branch review

- Update `tests/AGENTS.md` test categories and freshness date for the new tests (concurrency, errors, advertisement rotation).
- Update `test-peripheral/AGENTS.md` with the new characteristics, secondary service, opcode 0x04 behaviour, and payload size. Its "Four GATT services" line (~:26) is now four primary plus one secondary; also note the `SORT_BY_NAME` handle ordering.
- `tests/AGENTS.md`: the Discovery glob `test_discover_*.rs` misses `test_discovery_with_included_service.rs`.
- Run a final `opus` review over the whole range of commits from Step 1 to here, and a full hardware run of `./scripts/run-integration-tests.sh` on macOS with the nRF52840. Record any platform that could not be run.

**Commit:** `doc: Update test suite and test peripheral docs for regression expansion`

## Future Work

- **Bumble as hardware-free Linux CI.** The only strong reason to bring Bumble back: a Bumble virtual controller exposed to BlueZ through `hci_vhci`, with the virtual peripheral on the same virtual link, could run the suite against the BlueZ backend without a radio, possibly in GitHub Actions. Needs its own design plan; restore the old implementation from the Step 2 commit's parent as a starting point, fix its advertising layout, and bring it to parity with the Zephyr profile from this plan.
- **Service Changed / `DeviceServicesModified`.** Requires bonding support in the firmware so Zephyr sends Service Changed indications.
