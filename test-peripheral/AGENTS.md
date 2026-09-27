# test-peripheral/ -- BLE Test Peripheral Firmware (Zephyr)

Freshness: 2026-09-27

## Purpose

Zephyr firmware GATT test peripheral used by the integration tests in `tests/`. The Bumble virtual peripheral was removed in the commit that introduced this file and can be restored from git history.

## Structure

- `zephyr/` -- C firmware for Zephyr RTOS (nRF52840 DK, ESP32-S3 DevKitC)
  - `src/gatt_profile.h` -- UUIDs, Control Point opcodes, test constants, shared `peripheral_state`
  - `src/gatt_profile.c` -- GATT service attribute tables (`BT_GATT_SERVICE_DEFINE`)
  - `src/control_service.c` -- Control Point dispatch, periodic notify/indicate, reset, delayed disconnect
  - `src/test_handlers.c` -- read/write/descriptor/CCC callbacks
  - `src/main.c` -- advertising and scan response data, connection callbacks, advertising restart
  - `boards/*.conf` -- per-board config (nRF52840 uses Segger RTT logging)
  - `west.yml` -- west T2 manifest pinning Zephyr v4.4.2 (SDK 1.0.1)
- `.venv/` -- gitignored uv venv
- `.west/` -- gitignored west workspace (setup in `README.md`)
- `deps/` -- gitignored Zephyr v4.4.2 tree and modules (hal_nordic, hal_espressif, cmsis_6, mbedtls, tf-psa-crypto, segger)

## Contracts

- The canonical UUID source of truth is `tests/common/gatt_uuids.rs`, mirrored in `zephyr/src/gatt_profile.h`.
- Five profile GATT services (plus Zephyr's built-in GAP/GATT services): four primary services (Control `0x0001`, Read/Write `0x0002`, Notification `0x0003`, Descriptor `0x0004`) and one secondary service (Included `0x0005`). The Included service is included by the Descriptor service.
- Control service characteristics: Control Point `0x0101` (write; opcodes below), Control Response `0x0102` (notify; never sent).
- Read/Write service characteristics: Static Read `0x0201` (read-only `[0x01, 0x02, 0x03, 0x04]`), Counter Read `0x0202` (read-only, increments on each read, u32 LE, 0 after reset), Write With Response `0x0203` (write-only; value not readable), Write Without Response `0x0204` (write-without-response only; stores into the Read/Write `0x0205` value and updates Write Log), Read/Write `0x0205`, Long Value `0x0206` (read/write, up to 512 bytes; empty after reset), Error Char `0x0207` (read and write always fail with ATT application error `0x80`), Write Log Char `0x0208` (read-only `[count_lo, count_hi, last_seq, out_of_order]` for write-without-response ordering).
- Notification service characteristics: Notify `0x0301`, Indicate `0x0302`, Configurable Notify `0x0303`, Refused Notify `0x0304` (its CCC is an unmanaged descriptor: reads `0x0000`, every write rejected with Write Request Rejected).
- Descriptor service: include declaration for the Included service at `attrs[1]`, then Descriptor Test Char `0x0401` (read-only `[0x00]`, with Read-Only Descriptor `0x04A1` `[DE AD BE EF]` and Read/Write Descriptor `0x04A2`).
- Included service: Included Char `0x0501` (read-only `[0x05]`).
- Control Point opcodes: `0x01` start notifications, `0x02` stop, `0x03` disconnect, `0x04` change adverts (sets a pending flag; the alternate set — flags + name advertising data, 128-bit service data + manufacturer data scan response — is used on the next advertising restart after a disconnect, and the flag clears on the next `connected()` callback or `0x05`), `0x05` reset, `0x06` set notification payload (truncated to 244 bytes, `CONFIG_BT_L2CAP_TX_MTU` minus the 3-byte ATT header; a payload longer than the negotiated MTU minus 3 is not sent).
- Peripheral advertises as `"btleplug-test"` with the Control Service UUID in the scan response. After `0x04` and a disconnect, the next advertising cycle instead carries 128-bit service data (Control Service UUID + `[0x01]`) with no service UUID list, reverting to the default set once a client connects and disconnects again.

## Invariants

- Adding a new GATT characteristic requires updating both `tests/common/gatt_uuids.rs` and `zephyr/src/gatt_profile.h`.
- New characteristics are appended to the end of existing services so the hardcoded `notify_svc.attrs[]` indices in `control_service.c` stay valid, and update the index table comment in `periodic_notify_handler` (`control_service.c`) when `notify_svc` grows.
- Static GATT services get handles in `SORT_BY_NAME` order of their `BT_GATT_SERVICE_DEFINE` variable names (control, descriptor, included, notify, rw), not source order. Zephyr's `_1_gatt_svc`/`_2_gap_svc` sort first and take the lowest handles. Only relative `attrs[]` indices within a service (as `control_service.c` uses for `notify_svc`) are safe to hardcode.
