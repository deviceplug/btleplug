# btleplug Integration Test Peripheral

This directory contains the Zephyr firmware for a BLE test peripheral used by btleplug's integration test suite. It exposes a GATT profile that the Rust tests in `tests/` exercise.

## Quick Start

### Zephyr hardware setup

The Zephyr firmware supports multiple boards, built from a pinned Zephyr `v4.4.2` tree so builds are reproducible. Pick whichever board you have.

**Prerequisites:**
- `uv` (used below to create an isolated Python environment for `west` and Zephyr's build tooling — never system `pip`)
- Python 3.12+ (Zephyr v4.4.2 requires it; `uv venv --python 3.12` below provisions it for you even if your system Python is older)
- Host build tools — CMake ≥ 3.20.5, Ninja, and `dtc` (the devicetree compiler):
  - macOS: `brew install cmake ninja dtc`
  - Debian/Ubuntu: `apt install cmake ninja-build device-tree-compiler`
  - See also Zephyr's [Install dependencies](https://docs.zephyrproject.org/latest/develop/getting_started/index.html) guide.
- One of the supported boards:
  - [nRF52840 DK](https://www.nordicsemi.com/Products/Development-hardware/nRF52840-DK) — also needs [nRF Command Line Tools](https://www.nordicsemi.com/Products/Development-tools/nRF-Command-Line-Tools)
  - [ESP32-S3 DevKitC](https://docs.espressif.com/projects/esp-dev-kits/en/latest/esp32s3/esp32-s3-devkitc-1/)

**Set up the pinned workspace (once, from `test-peripheral/`):**

```bash
uv venv --python 3.12 .venv
uv pip install --python .venv/bin/python west
source .venv/bin/activate
# fish: source .venv/bin/activate.fish

west init -l zephyr
west update
# Prevent an exported ZEPHYR_BASE from overriding the pinned tree above:
west config zephyr.base-prefer configfile

# Now that deps/zephyr exists, install its build and flash requirements:
uv pip install --python .venv/bin/python \
  -r deps/zephyr/scripts/requirements-base.txt \
  -r deps/modules/hal/espressif/zephyr/requirements.txt

# One-time Zephyr toolchain install (v4.4.2 requires SDK 1.0.1; installs to
# ~/zephyr-sdk-1.0.1 by default):
west sdk install --version 1.0.1 -t arm-zephyr-eabi xtensa-espressif_esp32s3_zephyr-elf

# ESP32-S3 also needs its HAL blob libraries fetched once:
west blobs fetch hal_espressif
```

This creates `test-peripheral/.west/` (workspace metadata) and `test-peripheral/deps/` (the pinned Zephyr `v4.4.2` tree and its modules — `hal_nordic`, `hal_espressif`, `cmsis_6`, `mbedtls`, `tf-psa-crypto`, `segger`), both gitignored along with `.venv/`. `test-peripheral/zephyr/west.yml` is the manifest; after editing it, re-run `west update` from `test-peripheral/` with `.venv` activated.

**Migrating from an older checkout:** if you previously ran `west init` inside `test-peripheral/zephyr/` (the pre-pinned layout), delete `test-peripheral/zephyr/.west`, `test-peripheral/zephyr/zephyr`, `test-peripheral/zephyr/modules`, `test-peripheral/zephyr/bootloader`, and `test-peripheral/zephyr/tools` before following the setup above.

**Build and flash (from `test-peripheral/zephyr/`, with `.venv` activated):**

```bash
# nRF52840 DK
west build -b nrf52840dk/nrf52840 --pristine
west flash --runner nrfjprog

# ESP32-S3 DevKitC
west build -b esp32s3_devkitc/esp32s3/procpu --pristine -d build-esp32s3
west flash -d build-esp32s3
```

The board boots and immediately starts advertising as `"btleplug-test"`.

**Run integration tests:**

```bash
# From the btleplug repo root:
cargo test --test '*' -- --ignored
```

## GATT Test Profile

The Zephyr firmware implements the test GATT profile. The canonical UUID definitions are in `tests/common/gatt_uuids.rs` (Rust) and mirrored in `zephyr/src/gatt_profile.h` (C).

### Services

| Service | UUID | Purpose |
|---------|------|---------|
| Control Service | `00000001-b5a3-f393-e0a9-e50e24dcca9e` | Command interface to control peripheral behavior |
| Read/Write Test | `00000002-b5a3-f393-e0a9-e50e24dcca9e` | Read and write characteristic operations |
| Notification Test | `00000003-b5a3-f393-e0a9-e50e24dcca9e` | Notify and indicate operations |
| Descriptor Test | `00000004-b5a3-f393-e0a9-e50e24dcca9e` | Descriptor read/write operations |
| Included (secondary) | `00000005-b5a3-f393-e0a9-e50e24dcca9e` | Included Char `00000501-...` (read-only `[0x05]`); included from the Descriptor service |

### Control Commands

Write these opcodes to the Control Point characteristic (`00000101-...`):

| Opcode | Command | Effect |
|--------|---------|--------|
| `0x01` | Start Notifications | Begin periodic notifications (1 Hz) |
| `0x02` | Stop Notifications | Stop all periodic notifications |
| `0x03` | Trigger Disconnect | Peripheral disconnects after 500ms |
| `0x04` | Change Advertisements | Sets a pending flag; on the next advertising restart after a disconnect, the peripheral advertises an alternate set (flags + name advertising data, 128-bit service data + manufacturer data scan response) instead of the default. Cleared by the next connection or by `0x05` |
| `0x05` | Reset State | Stop notifications, clear all buffers |
| `0x06` | Set Notification Payload | Remaining bytes become the notification payload, truncated to 244 bytes (`CONFIG_BT_L2CAP_TX_MTU` minus the 3-byte ATT header) |

## Environment Variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `BTLEPLUG_TEST_PERIPHERAL` | `btleplug-test` | Override the peripheral name to discover |

## Troubleshooting

### Tests can't find the peripheral

1. **Verify the peripheral is advertising:** Use a BLE scanner app (nRF Connect, LightBlue) to confirm `"btleplug-test"` appears.
2. **Check Bluetooth adapter:** Ensure your host has a working BLE adapter. On Linux, run `hciconfig` or `bluetoothctl show`.
3. **Check permissions:** On Linux, you may need to run tests with `sudo` or add your user to the `bluetooth` group.
4. **Increase scan timeout:** If the peripheral takes a while to appear, `DEFAULT_SCAN_TIMEOUT` (15 s) in `tests/common/peripheral_finder.rs` may need extending.

### Zephyr build fails

1. **Verify the pinned workspace is active and intact:**
   - `.venv` is activated (`source test-peripheral/.venv/bin/activate`, or `.venv/bin/activate.fish` under fish).
   - `west topdir` prints `.../test-peripheral`.
   - `west list zephyr` shows `deps/zephyr v4.4.2`.
   - `echo $ZEPHYR_BASE` is empty, or `west config zephyr.base-prefer` prints `configfile` (see "Stray `ZEPHYR_BASE`" below).
   - `west sdk list` shows `1.0.1` installed.
2. **Verify board target:** Board targets use slashes, not underscores (e.g. `nrf52840dk/nrf52840`, `esp32s3_devkitc/esp32s3/procpu`).
3. **Clean build:** `west build -b <board> --pristine`
4. **ESP32 first build:** The Espressif HAL is large — first build takes significantly longer than subsequent builds. This is normal.

### Stray `ZEPHYR_BASE`

If your shell (or another Zephyr workspace) exports `ZEPHYR_BASE`, `west` can resolve against that tree instead of the pinned `deps/zephyr` here. `west config zephyr.base-prefer configfile` (run once as part of setup above) makes `west` always prefer the workspace's own manifest over an exported `ZEPHYR_BASE`.

