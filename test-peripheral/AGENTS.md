# test-peripheral/ -- BLE Test Peripheral Firmware (Zephyr)

Freshness: 2026-09-26

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
- Four GATT services: Control (`0x0001`), Read/Write (`0x0002`), Notification (`0x0003`), Descriptor (`0x0004`).
- Control Point opcodes: `0x01` start notifications, `0x02` stop, `0x03` disconnect, `0x04` change adverts (sets a pending flag; the alternate set — flags + name advertising data, 128-bit service data + manufacturer data scan response — is used on the next advertising restart after a disconnect, and the flag clears on the next `connected()` callback or `0x05`), `0x05` reset, `0x06` set notification payload.
- Peripheral advertises as `"btleplug-test"` with the Control Service UUID in the scan response. After `0x04` and a disconnect, the next advertising cycle instead carries 128-bit service data (Control Service UUID + `[0x01]`) with no service UUID list, reverting to the default set once a client connects and disconnects again.

## Invariants

- Adding a new GATT characteristic requires updating both `tests/common/gatt_uuids.rs` and `zephyr/src/gatt_profile.h`.
- New characteristics are appended to the end of existing services so the hardcoded `notify_svc.attrs[]` indices in `control_service.c` stay valid, and update the index table comment in `periodic_notify_handler` (`control_service.c`) when `notify_svc` grows.
